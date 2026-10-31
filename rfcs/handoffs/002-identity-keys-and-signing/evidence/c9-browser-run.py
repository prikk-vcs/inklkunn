#!/usr/bin/env python3
"""The real-browser check of handoff 002's C9 (follow-up F2): the OpenID Connect round trip survives the browser.

What it does. It starts (1) the real inklkunn router and authenticator on a real port, by running the ignored test
`the_server_the_real_browser_run_drives` of crates/inklkunn-web/tests/signin/browser.rs (the provider's server side,
discovery and the token endpoint, is a stand-in inside that process); (2) the stand-in provider's browser side, an
HTTPS server of this script with a self-signed certificate for `idp.example`; and (3) a headless Chromium, which
it drives over the DevTools protocol with a small client written here in the standard library (a WebSocket client
and a request/response matcher). The wiki is `http://localhost:<port>`; the provider is `https://idp.example:<port>`
(Chromium is told to resolve that name to 127.0.0.1 and to accept the certificate): two different sites, so the
provider's redirect back to the wiki is a real cross-site top-level navigation. `localhost` is a secure context for
Chromium, so the `Secure` and `__Host-` cookies the server sets are stored without TLS.

What it checks, in Chromium's own words (Network.requestWillBeSentExtraInfo reports, per request, which cookies
were sent and which were blocked and why):

  S1  the whole round trip: the login-state cookie (Lax, HttpOnly, Secure, short-lived) is stored when the browser
      is sent to the provider, and IS SENT on the provider's cross-site redirect back; the answer is a 200 page
      (not a redirect) that the browser follows with a request of the wiki's own, which carries the Strict session
      cookie; the person is signed in on the page it lands on; the state cookie is gone afterwards (single use);
      no Referer carries the callback's address (its query holds the provider's code) to the landing page.
  S2  why the landing is a page: a cross-site-initiated navigation to the wiki is sent WITHOUT the Strict session
      cookie (Chromium blocks it as SameSiteStrictCookie), so a redirect after the callback would have landed the
      person signed out; a same-site navigation made right after carries it.
  S3  login CSRF: a callback address with a state this browser did not start (carrying another browser's state, in
      a browser that has no login-state cookie) is refused (403), signs nobody in and sets no session; the browser
      that did start that login can still finish it.
  S4  replay: the callback of S1 loaded again, with the old state cookie put back by hand, is refused: the server
      forgot the state when it was used.

What it does NOT cover: other browser engines (Firefox, Safari) and other versions; a real identity provider and
real TLS in front of the wiki; the ID token's signature and claims (the auth crate's own tests, with a stand-in
verifier here); the `__Host-` prefix over plain http beyond Chromium's `localhost` rule; any mobile browser.

The run can fail: `c9-browser-run.mutated-strict-state-cookie.out.txt` is the same script run against a build whose
login-state cookie was changed (for this one run, then put back) to `SameSite=Strict`; the browser then does not carry
the cookie back and the round trip never signs anybody in (6 checks fail, the rest of the report says where).
`c9-browser-run.out.txt` is the run against the committed code.

Usage: python3 c9-browser-run.py [--out FILE] [--chromium PATH]   (from anywhere in the repository; needs cargo,
openssl and chromium on PATH; no network). Exit status 0 only if every check passed.
"""

import base64
import hashlib
import http.server
import json
import os
import platform
import re
import socket
import ssl
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
STATE_COOKIE = "__Host-inklkunn-oidc"
SESSION_COOKIE = "__Host-inklkunn-session"

LINES = []
CHECKS = []


def log(text=""):
    print(text, flush=True)
    LINES.append(text)


def check(name, ok, detail=""):
    CHECKS.append((name, bool(ok)))
    log(f"  [{'PASS' if ok else 'FAIL'}] {name}" + (f"  ({detail})" if detail else ""))
    return bool(ok)


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


# ------------------------------------------------------------------------------------ a WebSocket client


class WebSocket:
    """Just enough of RFC 6455 for the DevTools protocol: a client handshake, masked text frames out, text frames
    in (fragmented or not), ping answered, close noticed."""

    def __init__(self, url):
        u = urllib.parse.urlparse(url)
        self.sock = socket.create_connection((u.hostname, u.port), timeout=30)
        key = base64.b64encode(os.urandom(16)).decode()
        self.sock.sendall(
            (
                f"GET {u.path} HTTP/1.1\r\nHost: {u.hostname}:{u.port}\r\nUpgrade: websocket\r\n"
                f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
            ).encode()
        )
        self.buf = b""
        while b"\r\n\r\n" not in self.buf:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise RuntimeError("the DevTools socket closed during the handshake")
            self.buf += chunk
        head, self.buf = self.buf.split(b"\r\n\r\n", 1)
        if b" 101 " not in head.split(b"\r\n", 1)[0]:
            raise RuntimeError(f"no WebSocket upgrade: {head[:200]!r}")
        accept = base64.b64encode(
            hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest()
        ).decode()
        if accept.encode() not in head:
            raise RuntimeError("the server's Sec-WebSocket-Accept is wrong")

    def _exact(self, n):
        while len(self.buf) < n:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise ConnectionError("the DevTools socket closed")
            self.buf += chunk
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def send(self, text):
        data = text.encode()
        head = bytes([0x81])
        n = len(data)
        if n < 126:
            head += bytes([0x80 | n])
        elif n < 65536:
            head += bytes([0x80 | 126]) + struct.pack(">H", n)
        else:
            head += bytes([0x80 | 127]) + struct.pack(">Q", n)
        mask = os.urandom(4)
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(data))
        self.sock.sendall(head + mask + masked)

    def recv(self, timeout):
        """The next text message, or None if none arrives within `timeout` seconds."""
        self.sock.settimeout(timeout)
        try:
            message = b""
            while True:
                b0, b1 = self._exact(2)
                opcode, fin = b0 & 0x0F, bool(b0 & 0x80)
                n = b1 & 0x7F
                if n == 126:
                    n = struct.unpack(">H", self._exact(2))[0]
                elif n == 127:
                    n = struct.unpack(">Q", self._exact(8))[0]
                payload = self._exact(n)
                if opcode == 0x8:
                    raise ConnectionError("the DevTools socket was closed by the browser")
                if opcode == 0x9:
                    self.sock.sendall(bytes([0x8A, 0x80]) + os.urandom(4))
                    continue
                if opcode in (0x1, 0x0):
                    message += payload
                    if fin:
                        return message.decode()
        except socket.timeout:
            return None
        finally:
            self.sock.settimeout(30)


class Cdp:
    """DevTools commands with their answers, and the events that arrive meanwhile, kept in order."""

    def __init__(self, ws):
        self.ws = ws
        self.next_id = 0
        self.events = []

    def _take(self, raw):
        message = json.loads(raw)
        if "id" not in message:
            self.events.append(message)
        return message

    def call(self, method, params=None, session=None, timeout=30):
        self.next_id += 1
        mine = self.next_id
        request = {"id": mine, "method": method, "params": params or {}}
        if session:
            request["sessionId"] = session
        self.ws.send(json.dumps(request))
        deadline = time.time() + timeout
        while time.time() < deadline:
            raw = self.ws.recv(0.5)
            if raw is None:
                continue
            message = self._take(raw)
            if message.get("id") == mine:
                if "error" in message:
                    raise RuntimeError(f"{method}: {message['error']}")
                return message.get("result", {})
        raise TimeoutError(method)

    def pump(self, seconds):
        deadline = time.time() + seconds
        while time.time() < deadline:
            raw = self.ws.recv(0.1)
            if raw is not None:
                self._take(raw)

    def wait_until(self, condition, timeout=20, what="a condition"):
        deadline = time.time() + timeout
        while time.time() < deadline:
            if condition():
                return True
            self.pump(0.1)
        raise TimeoutError(what)


class Tab:
    """One tab in its own browser context (its own cookie jar), with what the network did since a mark."""

    def __init__(self, cdp):
        self.cdp = cdp
        self.context = cdp.call("Target.createBrowserContext")["browserContextId"]
        target = cdp.call("Target.createTarget", {"url": "about:blank", "browserContextId": self.context})
        self.target = target["targetId"]
        self.session = cdp.call("Target.attachToTarget", {"targetId": self.target, "flatten": True})["sessionId"]
        for domain in ("Page", "Network", "Runtime"):
            cdp.call(f"{domain}.enable", session=self.session)
        self.mark = 0

    def call(self, method, params=None):
        return self.cdp.call(method, params, session=self.session)

    def location(self):
        return self.call("Runtime.evaluate", {"expression": "location.href", "returnByValue": True})["result"].get("value", "")

    def text(self):
        return self.call(
            "Runtime.evaluate",
            {"expression": "document.body ? document.body.innerText : ''", "returnByValue": True},
        )["result"].get("value", "")

    def html(self):
        return self.call(
            "Runtime.evaluate",
            {"expression": "document.documentElement.outerHTML", "returnByValue": True},
        )["result"].get("value", "")

    def evaluate(self, expression):
        return self.call("Runtime.evaluate", {"expression": expression, "returnByValue": True})

    def navigate(self, url):
        self.call("Page.navigate", {"url": url})

    def settle(self, url_part, timeout=20):
        """Wait until the tab's address contains `url_part` and the page there has loaded."""
        self.cdp.wait_until(
            lambda: url_part in self.location()
            and self.evaluate("document.readyState")["result"].get("value") == "complete",
            timeout,
            f"the tab to reach {url_part}",
        )
        self.cdp.pump(0.3)

    def cookies(self):
        """Every cookie of this tab's browser context, with the attributes the browser holds them with."""
        return self.cdp.call("Storage.getCookies", {"browserContextId": self.context})["cookies"]

    def mark_now(self):
        self.mark = len(self.cdp.events)

    def requests(self):
        """What the network did since the mark, in order: one entry per request attempt (a redirect is another).
        Chromium reports a request's cookies and its response's headers in separate "extra info" events, in the
        order of the attempts of the same request id, so they are paired by that order."""
        entries, by_rid = [], {}
        sent_info, response_info, final = {}, {}, {}
        for event in self.cdp.events[self.mark:]:
            if event.get("sessionId") != self.session:
                continue
            method, params = event.get("method"), event.get("params", {})
            rid = params.get("requestId")
            if method == "Network.requestWillBeSent":
                request = params["request"]
                entry = {
                    "method": request["method"],
                    "url": request["url"],
                    "referer": request.get("headers", {}).get("Referer"),
                    "type": params.get("type"),
                    "status": None,
                    "location": None,
                    "sent": None,
                    "blocked": {},
                    "set": [],
                    "set_lines": [],
                }
                entries.append(entry)
                by_rid.setdefault(rid, []).append(entry)
            elif method == "Network.requestWillBeSentExtraInfo":
                sent_info.setdefault(rid, []).append(params)
            elif method == "Network.responseReceivedExtraInfo":
                response_info.setdefault(rid, []).append(params)
            elif method == "Network.responseReceived":
                final[rid] = params["response"]
        for rid, attempts in by_rid.items():
            for i, entry in enumerate(attempts):
                if i < len(sent_info.get(rid, [])):
                    sent, blocked = [], {}
                    for associated in sent_info[rid][i].get("associatedCookies", []):
                        name = associated["cookie"]["name"]
                        reasons = associated.get("blockedReasons", [])
                        if reasons:
                            blocked[name] = reasons
                        else:
                            sent.append(name)
                    entry["sent"], entry["blocked"] = sent, blocked
                if i < len(response_info.get(rid, [])):
                    info = response_info[rid][i]
                    headers = {k.lower(): v for k, v in info.get("headers", {}).items()}
                    entry["status"] = info.get("statusCode")
                    entry["location"] = headers.get("location")
                    lines = [line for line in headers.get("set-cookie", "").split("\n") if line.strip()]
                    entry["set_lines"] = lines
                    entry["set"] = [line.split("=", 1)[0] for line in lines]
                elif i == len(attempts) - 1 and rid in final:
                    entry["status"] = final[rid].get("status")
        return [e for e in entries if e["type"] in ("Document", None) or e["method"] == "POST"]


def show(entries):
    for e in entries:
        sent = "-" if e["sent"] is None else (",".join(e["sent"]) or "none")
        blocked = ";".join(f"{k}:{'/'.join(v)}" for k, v in e["blocked"].items())
        sets = ",".join(e["set"])
        log(
            f"      {e['method']:4} {redact(e['url'])} -> {e['status']}"
            f"  cookies sent: {sent}" + (f"  blocked: {blocked}" if blocked else "") + (f"  set: {sets}" if sets else "")
            + (f"  referer: {redact(e['referer'])}" if e["referer"] else "")
        )


def redact(url):
    """A state, a nonce and a proof key in an address are one-time values; show that there are some and not what
    they are."""
    if not url:
        return url
    return re.sub(r"(state|nonce|code_challenge)=([^&]{4})[^&]*", r"\1=\2…", url)


# ------------------------------------------------------------------------------------ the stand-in provider


class Provider:
    """The browser side of a provider: an authorize page with an Approve button (a POST answered with a 302 back
    to the wiki, as a real provider's login is) and a page of its own with a link into the wiki."""

    def __init__(self, port, cert, key, app_origin):
        self.seen = []
        provider = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def page(self, body, status=200):
                data = f"<!doctype html><html><head><meta charset=utf-8><title>idp</title></head><body>{body}</body></html>".encode()
                self.send_response(status)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def do_GET(self):
                url = urllib.parse.urlparse(self.path)
                query = urllib.parse.parse_qs(url.query)
                provider.seen.append({"path": url.path, "referer": self.headers.get("Referer"), "cookie": self.headers.get("Cookie")})
                if url.path == "/authorize":
                    form = (
                        '<form method="post" action="/approve">'
                        f'<input type="hidden" name="redirect_uri" value="{query["redirect_uri"][0]}">'
                        f'<input type="hidden" name="state" value="{query["state"][0]}">'
                        '<button id="approve" type="submit">Approve</button></form>'
                    )
                    self.page(f"<h1>Stand-in provider</h1>{form}")
                elif url.path == "/elsewhere":
                    self.page(f'<a id="into-the-wiki" href="{app_origin}/-/account">into the wiki</a>')
                else:
                    self.page("not found", 404)

            def do_POST(self):
                length = int(self.headers.get("Content-Length", "0"))
                form = urllib.parse.parse_qs(self.rfile.read(length).decode())
                provider.seen.append({"path": self.path, "referer": self.headers.get("Referer"), "cookie": self.headers.get("Cookie")})
                if self.path == "/approve":
                    back = form["redirect_uri"][0]
                    state = urllib.parse.quote(form["state"][0], safe="")
                    self.send_response(302)
                    self.send_header("Location", f"{back}?code=stand-in-code&state={state}")
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                else:
                    self.page("not found", 404)

        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(cert, key)
        self.httpd.socket = context.wrap_socket(self.httpd.socket, server_side=True)
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()

    def stop(self):
        self.httpd.shutdown()
        self.httpd.server_close()


# ------------------------------------------------------------------------------------ the run


def main():
    out_path = os.path.join(HERE, "c9-browser-run.out.txt")
    chromium = "chromium"
    args = sys.argv[1:]
    while args:
        flag = args.pop(0)
        if flag == "--out":
            out_path = args.pop(0)
        elif flag == "--chromium":
            chromium = args.pop(0)
        else:
            sys.exit(f"unknown argument {flag}")

    scratch = tempfile.mkdtemp(prefix="inklkunn-c9-")
    app_port, idp_port, cdp_port = free_port(), free_port(), free_port()
    app_origin = f"http://localhost:{app_port}"
    idp_origin = f"https://idp.example:{idp_port}"
    stop_file = os.path.join(scratch, "stop")
    children = []
    provider = None
    try:
        commit = subprocess.run(["git", "-C", REPO, "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
        log("C9 real-browser run (handoff 002, follow-up F2)")
        log(f"  date        {time.strftime('%Y-%m-%d %H:%M:%S %Z')}")
        log(f"  repository  {commit or 'unknown'}")
        log(f"  host        {platform.platform()}; python {platform.python_version()}")

        cert, key = os.path.join(scratch, "idp.pem"), os.path.join(scratch, "idp.key")
        subprocess.run(
            [
                "openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", key, "-out", cert,
                "-days", "2", "-subj", "/CN=idp.example", "-addext", "subjectAltName=DNS:idp.example",
            ],
            check=True, capture_output=True,
        )
        provider = Provider(idp_port, cert, key, app_origin)

        env = dict(os.environ)
        env.update(
            INKLKUNN_BROWSER_APP_PORT=str(app_port),
            INKLKUNN_BROWSER_IDP=idp_origin,
            INKLKUNN_BROWSER_STOP_FILE=stop_file,
        )
        server = subprocess.Popen(
            [
                "cargo", "test", "-p", "inklkunn-web", "--locked", "--test", "signin", "--",
                "--ignored", "--nocapture", "--exact", "browser::the_server_the_real_browser_run_drives",
            ],
            cwd=REPO, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
        )
        children.append(server)
        listening = threading.Event()
        server_lines = []

        def read_server():
            for line in server.stdout:
                server_lines.append(line.rstrip())
                if line.startswith("LISTENING"):
                    listening.set()

        threading.Thread(target=read_server, daemon=True).start()
        if not listening.wait(900):
            sys.exit("the server did not come up:\n" + "\n".join(server_lines[-20:]))
        log(f"  server      the inklkunn router and authenticator on {app_origin} (cargo test, ignored test browser::…)")

        profile = os.path.join(scratch, "profile")
        browser = subprocess.Popen(
            [
                chromium, "--headless=new", f"--remote-debugging-port={cdp_port}", f"--user-data-dir={profile}",
                "--no-first-run", "--disable-gpu", "--ignore-certificate-errors",
                "--host-resolver-rules=MAP idp.example 127.0.0.1", "about:blank",
            ],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        children.append(browser)
        version = None
        for _ in range(100):
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{cdp_port}/json/version", timeout=2) as response:
                    version = json.load(response)
                break
            except OSError:
                time.sleep(0.2)
        if not version:
            sys.exit("chromium did not answer on the DevTools port")
        log(f"  browser     {version['Browser']} ({version['User-Agent']})")
        log(f"  sites       wiki {app_origin}  provider {idp_origin}  (cross-site)")
        log("")

        cdp = Cdp(WebSocket(version["webSocketDebuggerUrl"]))

        a = callback_url = old_state = None
        # ---------------------------------------------------------------- S1: the whole round trip
        try:
            log("S1  sign in with the provider, as a person does")
            a = Tab(cdp)
            a.navigate(f"{app_origin}/-/sign-in")
            a.settle("/-/sign-in")
            link = a.evaluate("document.querySelector('a[href^=\"/-/sign-in/oidc\"]') ? document.querySelector('a[href^=\"/-/sign-in/oidc\"]').textContent : null")["result"].get("value")
            check("the sign-in page offers the provider", link is not None and "idp.example" in link, repr(link))
            a.mark_now()
            a.evaluate("document.querySelector('a[href^=\"/-/sign-in/oidc\"]').click()")
            a.settle("/authorize")
            jar = {c["name"]: c for c in a.cookies()}
            state_cookie = jar.get(STATE_COOKIE)
            check("the login-state cookie is stored when the browser is sent away", state_cookie is not None)
            if state_cookie:
                lifetime = state_cookie["expires"] - time.time() if state_cookie.get("expires", -1) > 0 else None
                check("  ... SameSite=Lax", state_cookie.get("sameSite") == "Lax", state_cookie.get("sameSite"))
                check("  ... HttpOnly", state_cookie.get("httpOnly") is True)
                check("  ... Secure", state_cookie.get("secure") is True)
                check("  ... short-lived (a few minutes, not a session or a day)", lifetime is not None and 0 < lifetime <= 600, f"{lifetime and round(lifetime)}s left")
            check("no session exists yet", SESSION_COOKIE not in jar)
            authorize = [s for s in provider.seen if s["path"] == "/authorize"]
            check(
                "the provider was sent no Referer by the wiki (the wiki's address does not travel)",
                authorize and not authorize[-1]["referer"],
                repr(authorize[-1]["referer"] if authorize else None),
            )
            check("the provider saw none of the wiki's cookies", authorize and not authorize[-1]["cookie"])
            a.evaluate("document.querySelector('#approve').click()")
            a.settle("/-/account")
            hops = a.requests()
            show(hops)
            callback = next((h for h in hops if "/-/sign-in/oidc/callback" in h["url"]), None)
            check("the provider's redirect back reached the callback", callback is not None)
            if callback:
                check("the callback was answered 200 (a page, not a redirect)", callback["status"] == 200, callback["status"])
                check("the login-state cookie was SENT on that cross-site redirect (it is Lax)", callback["sent"] is not None and STATE_COOKIE in callback["sent"], callback["sent"])
                check("the callback set the session and removed the state cookie", set(callback["set"]) >= {SESSION_COOKIE, STATE_COOKIE}, callback["set"])
                session_line = next((l for l in callback.get("set_lines", []) if l.startswith(SESSION_COOKIE)), "")
                check("the session cookie is SameSite=Strict", "SameSite=Strict" in session_line, session_line.split(";", 1)[-1].strip())
                state_line = next((l for l in callback.get("set_lines", []) if l.startswith(STATE_COOKIE)), "")
                check("the state cookie's removal is Max-Age=0 and Lax", "Max-Age=0" in state_line and "SameSite=Lax" in state_line, state_line.split(";", 1)[-1].strip())
                after = hops[hops.index(callback) + 1:]
                landing = next((h for h in after if h["url"].endswith("/-/account")), None)
                check("the page then moved the browser on with a request of the wiki's own", landing is not None)
                if landing:
                    check("that request carried the Strict session cookie", landing["sent"] is not None and SESSION_COOKIE in landing["sent"], landing["sent"])
                    check("... without a Referer (the callback's address held the provider's code)", not landing["referer"], repr(landing["referer"]))
                    check("... and was answered 200, the account page", landing["status"] == 200, landing["status"])
            text = a.text()
            check("the person is signed in on the page they landed on", "Signed in as" in text, text.replace("\n", " | ")[:100])
            jar = {c["name"]: c for c in a.cookies()}
            check("the state cookie is gone afterwards (single use)", STATE_COOKIE not in jar)
            session = jar.get(SESSION_COOKIE)
            check(
                "the session cookie is stored: Strict, HttpOnly, Secure",
                session is not None and session.get("sameSite") == "Strict" and session.get("httpOnly") and session.get("secure"),
                session and session.get("sameSite"),
            )
            callback_url = callback["url"] if callback else None
            old_state = urllib.parse.parse_qs(urllib.parse.urlparse(callback_url).query)["state"][0] if callback_url else None
            log("")

        except (TimeoutError, RuntimeError, KeyError, ConnectionError) as e:
            check("S1 ran to its end", False, f"{type(e).__name__}: {e}")
            log("")
        # ---------------------------------------------------------------- S2: why the landing is a page
        try:
            log("S2  a cross-site navigation into the wiki does not carry the Strict session cookie")
            a.navigate(f"{idp_origin}/elsewhere")
            a.settle("/elsewhere")
            a.mark_now()
            a.evaluate("document.querySelector('#into-the-wiki').click()")
            a.settle("localhost")
            a.cdp.pump(0.5)
            hops = a.requests()
            show(hops)
            into = next((h for h in hops if h["url"].endswith("/-/account")), None)
            check("the request into the wiki from the provider's page was made", into is not None)
            if into:
                check(
                    "Chromium blocked the Strict session cookie on it (a same-site-strict block)",
                    SESSION_COOKIE in into["blocked"] and any("SameSiteStrict" in r for r in into["blocked"][SESSION_COOKIE]),
                    into["blocked"],
                )
                check("so the person would have landed signed out", "Signed in as" not in a.text(), a.location())
            a.mark_now()
            a.evaluate("location.assign('/-/account')")
            a.settle("/-/account")
            hops = a.requests()
            again = next((h for h in hops if h["url"].endswith("/-/account")), None)
            check(
                "a same-site navigation made right after carries it, and the person is signed in",
                again is not None and again["sent"] is not None and SESSION_COOKIE in again["sent"] and "Signed in as" in a.text(),
                again and again["sent"],
            )
            log("")

        except (TimeoutError, RuntimeError, KeyError, ConnectionError) as e:
            check("S2 ran to its end", False, f"{type(e).__name__}: {e}")
            log("")
        # ---------------------------------------------------------------- S3: login CSRF
        try:
            log("S3  a callback this browser did not start")
            attacker = Tab(cdp)
            attacker.navigate(f"{app_origin}/-/sign-in/oidc")
            attacker.settle("/authorize")
            attacker_state = urllib.parse.parse_qs(urllib.parse.urlparse(attacker.location()).query)["state"][0]
            victim = Tab(cdp)
            victim.mark_now()
            victim.navigate(f"{app_origin}/-/sign-in/oidc/callback?code=stand-in-code&state={urllib.parse.quote(attacker_state, safe='')}")
            victim.settle("/-/sign-in/oidc/callback")
            hops = victim.requests()
            show(hops)
            refused = next((h for h in hops if "/callback" in h["url"]), None)
            check("the victim's browser (no login-state cookie) is refused with 403", refused is not None and refused["status"] == 403, refused and refused["status"])
            check("... with the sentence that says the sign-in did not start here", "did not start in this browser" in victim.text(), victim.text().replace("\n", " | ")[:100])
            vjar = {c["name"]: c for c in victim.cookies()}
            check("... and no session was set", SESSION_COOKIE not in vjar)
            attacker.evaluate("document.querySelector('#approve').click()")
            cdp.wait_until(lambda: "Signed in as" in attacker.text(), 20, "the attacker's own login to finish")
            check("the browser that did start that login can still finish it (nothing was spent)", "Signed in as" in attacker.text(), attacker.text().replace("\n", " | ")[:60])
            log("")

        except (TimeoutError, RuntimeError, KeyError, ConnectionError) as e:
            check("S3 ran to its end", False, f"{type(e).__name__}: {e}")
            log("")
        # ---------------------------------------------------------------- S4: replay
        try:
            log("S4  the callback of S1 again, with its state cookie put back by hand")
            replay = Tab(cdp)
            replay.call(
                "Network.setCookie",
                {"name": STATE_COOKIE, "value": old_state, "url": app_origin, "path": "/", "secure": True, "httpOnly": True, "sameSite": "Lax"},
            )
            replay.mark_now()
            replay.navigate(callback_url)
            replay.settle("/-/sign-in/oidc/callback")
            hops = replay.requests()
            show(hops)
            again = next((h for h in hops if "/callback" in h["url"]), None)
            check("the browser sent the state cookie with the replayed address", again is not None and again["sent"] is not None and STATE_COOKIE in again["sent"], again and again["sent"])
            check("... and the server refused it, 403 (it forgot the state when it was used)", again is not None and again["status"] == 403, again and again["status"])
            check("... nobody was signed in", SESSION_COOKIE not in {c["name"] for c in replay.cookies()})
            log("")

        except (TimeoutError, RuntimeError, KeyError, ConnectionError) as e:
            check("S4 ran to its end", False, f"{type(e).__name__}: {e}")
            log("")
        # ---------------------------------------------------------------- the end
        failed = [name for name, ok in CHECKS if not ok]
        log(f"RESULT  {len(CHECKS) - len(failed)} of {len(CHECKS)} checks passed" + (f"; FAILED: {failed}" if failed else ""))
        log("")
        log("What this shows: in Chromium " + version["Browser"].split("/")[-1] + ", with the wiki and the provider on different sites, the login-state")
        log("cookie (Lax, HttpOnly, Secure, 10 minutes, removed by the answer that uses it) is carried back by the provider's")
        log("cross-site redirect, while the Strict session cookie is withheld from a cross-site navigation (S2); the 200 landing")
        log("page, which the browser follows with a request of the wiki's own, therefore signs the person in; a callback this")
        log("browser did not start, and a callback used twice, sign nobody in. It does not show other browsers or versions, a real")
        log("provider or TLS in front of the wiki, or the ID token's signature (see the header of this script).")
        return 1 if failed else 0
    finally:
        # Only what this run started: the server is asked to stop by its stop file, the browser and the provider by
        # their own handles.
        try:
            open(stop_file, "w").close()
        except OSError:
            pass
        if provider:
            provider.stop()
        for child in reversed(children):
            try:
                if child.poll() is None:
                    if child.args[0] == "cargo":
                        child.wait(timeout=20)
                    else:
                        child.terminate()
                        child.wait(timeout=10)
            except Exception:
                try:
                    child.terminate()
                except Exception:
                    pass
        with open(out_path, "w") as f:
            f.write("\n".join(LINES) + "\n")


if __name__ == "__main__":
    sys.exit(main())
