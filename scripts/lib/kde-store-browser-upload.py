#!/usr/bin/env python3
"""Upload a .plasmoid via the store Files UI (browser CDP), without leaking cookies.

Privacy contract:
  - Never print cookie values, OAuth codes, or session tokens.
  - Cookies are read only from a Netscape cookie file (mode 0600) written by kde-store-auth.py.
  - Isolated Chrome profile under /tmp; removed on exit.
  - Stdout: status lines only (ok / error codes / public file_id / version).

This path is used when POST /addpploadfile/ returns a generic JSON error (common)
while the Files dropzone UI still works.
"""

from __future__ import annotations

import asyncio
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request

PRODUCT_ID_DEFAULT = "2365275"
STORE_BASE_DEFAULT = "https://store.kde.org"


def status(msg: str) -> None:
    """Print a non-secret status line."""
    sys.stdout.write(msg + "\n")
    sys.stdout.flush()


def die(code: str, exit_code: int = 1) -> None:
    status(f"error:{code}")
    raise SystemExit(exit_code)


def parse_netscape(path: pathlib.Path) -> list[dict]:
    cookies: list[dict] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) < 7:
            continue
        domain, _inc, path_value, secure, _exp, name, value = parts[:7]
        entry: dict = {
            "name": name,
            "value": value,
            "path": path_value or "/",
            "secure": secure.upper() == "TRUE",
        }
        if domain.startswith("."):
            entry["domain"] = domain
        else:
            entry["url"] = f"https://{domain}{path_value or '/'}"
        cookies.append(entry)
    return cookies


async def cdp_call(ws, mid: list[int], method: str, params: dict | None = None, timeout: float = 120.0):
    import websockets  # noqa: F401 — imported by caller env

    mid[0] += 1
    msg_id = mid[0]
    await ws.send(json.dumps({"id": msg_id, "method": method, "params": params or {}}))
    while True:
        raw = await asyncio.wait_for(ws.recv(), timeout=timeout)
        data = json.loads(raw)
        if data.get("id") == msg_id:
            return data


async def eval_js(ws, mid: list[int], expression: str, await_promise: bool = False, timeout: float = 120.0):
    params = {"expression": expression, "returnByValue": True, "awaitPromise": await_promise}
    if await_promise:
        params["timeout"] = int(timeout * 1000)
    res = await cdp_call(ws, mid, "Runtime.evaluate", params, timeout=timeout)
    if res.get("result", {}).get("exceptionDetails"):
        die("js_exception")
    return res.get("result", {}).get("result", {}).get("value")


def wait_for_cdp(port: int, seconds: float = 15.0) -> dict:
    deadline = time.time() + seconds
    last_err = None
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/json/version", timeout=1.5) as resp:
                return json.load(resp)
        except Exception as exc:  # noqa: BLE001 — probe loop
            last_err = type(exc).__name__
            time.sleep(0.3)
    die(f"cdp_unavailable:{last_err or 'timeout'}")


def pick_page(port: int) -> dict:
    with urllib.request.urlopen(f"http://127.0.0.1:{port}/json/list", timeout=5) as resp:
        pages = json.load(resp)
    for page in pages:
        if page.get("type") == "page":
            return page
    die("no_cdp_page")


async def ensure_backend(ws, mid: list[int], edit_url: str) -> None:
    await cdp_call(ws, mid, "Page.navigate", {"url": edit_url})
    await asyncio.sleep(2.5)
    info = await eval_js(
        ws,
        mid,
        '({url: location.href, backend: /welcome to your store backend/i.test(document.body ? document.body.innerText : "")})',
    )
    if info and info.get("backend"):
        status("auth:backend")
        return

    # GitHub OAuth select-account / authorize (uses cookies already set for github.com if present).
    status("auth:oauth_start")
    await cdp_call(
        ws,
        mid,
        "Page.navigate",
        {"url": "https://www.opendesktop.org/oauth/external/login/github?action=login"},
    )
    await asyncio.sleep(2.5)
    for _attempt in range(10):
        state = await eval_js(
            ws,
            mid,
            "({url: location.href, backend: /welcome to your store backend/i.test(document.body ? document.body.innerText : \"\")})",
        )
        url = (state or {}).get("url") or ""
        if (state or {}).get("backend") or ("store.kde.org" in url and "/edit" in url):
            break
        await eval_js(
            ws,
            mid,
            """(() => {
              const want = /authorize|continue|select|confirm|accept/i;
              for (const el of document.querySelectorAll('button,a,input[type=submit]')) {
                const t = (el.innerText || el.value || '').trim();
                if (want.test(t)) { el.click(); return true; }
              }
              return false;
            })()""",
        )
        await asyncio.sleep(2)
    await cdp_call(ws, mid, "Page.navigate", {"url": edit_url})
    await asyncio.sleep(2.5)
    info = await eval_js(
        ws,
        mid,
        '({backend: /welcome to your store backend/i.test(document.body ? document.body.innerText : "")})',
    )
    if not info or not info.get("backend"):
        die("auth_failed_sign_in_chrome")
    status("auth:backend")


async def upload_via_files_ui(
    ws,
    mid: list[int],
    archive: pathlib.Path,
    version: str,
    product_id: str,
    store_base: str,
) -> str:
    edit_url = f"{store_base}/p/{product_id}/edit/"
    await ensure_backend(ws, mid, edit_url)

    # Open Files step
    await eval_js(
        ws,
        mid,
        'document.querySelector(\'a[href*="add-product-form-h-2"]\')?.click(); true',
    )
    await asyncio.sleep(1.0)

    # Accept terms
    await eval_js(
        ws,
        mid,
        """(() => {
          for (const c of document.querySelectorAll('input[type=checkbox]')) {
            const parentText = (c.closest('label,div,p,li,span') || c.parentElement || document.body).innerText || '';
            if (/accept the Terms/i.test(parentText)) {
              if (!c.checked) c.click();
              return true;
            }
          }
          return false;
        })()""",
    )

    # Set file on hidden dropzone input
    doc = await cdp_call(ws, mid, "DOM.getDocument", {"depth": -1})
    root = doc["result"]["root"]["nodeId"]
    q = await cdp_call(
        ws,
        mid,
        "DOM.querySelector",
        {"nodeId": root, "selector": "input[data-file-upload]"},
    )
    node_id = q.get("result", {}).get("nodeId")
    if not node_id:
        die("no_file_input")
    await cdp_call(
        ws,
        mid,
        "DOM.setFileInputFiles",
        {"nodeId": node_id, "files": [str(archive)]},
    )
    status("upload:file_selected")

    clicked = await eval_js(
        ws,
        mid,
        """(() => {
          const input = document.querySelector('input[data-file-upload]');
          if (input) input.dispatchEvent(new Event('change', {bubbles: true}));
          const add = [...document.querySelectorAll('button,a,input')].find(b =>
            /Add File\\(s\\)/i.test(b.innerText || b.value || '')
          );
          if (add) { add.click(); return true; }
          return false;
        })()""",
    )
    if not clicked:
        die("add_file_click_failed")
    status("upload:started")

    name = archive.name
    # Wait until the completed row shows an MD5 (not only the progress line).
    file_id = None
    md5 = None
    for _i in range(90):
        await asyncio.sleep(2)
        state = await eval_js(
            ws,
            mid,
            f"""(() => {{
              const name = {json.dumps(name)};
              const row = [...document.querySelectorAll('tr')].find(r => (r.innerText || '').includes(name));
              if (!row) return {{hasRow: false}};
              const id = row.getAttribute('data-ppload-file-id') || null;
              const text = row.innerText || '';
              const m = text.match(/\\b([a-f0-9]{{32}})\\b/i);
              return {{hasRow: true, id, md5: m && m[1], text: text.slice(0, 160)}};
            }})()""",
        )
        if state and state.get("md5") and state.get("id"):
            file_id = str(state["id"])
            md5 = state["md5"]
            break
    if not file_id or not md5:
        die("upload_timeout")
    # Public identifiers only — md5 of the release artifact is not a secret.
    status(f"upload:complete file_id={file_id} md5={md5}")

    # Tag file version + OCS compatible via product endpoint (works when session is live).
    update = await eval_js(
        ws,
        mid,
        f"""(async () => {{
          const form = new FormData();
          form.append('file_id', {json.dumps(file_id)});
          form.append('file_version', {json.dumps(version)});
          form.append('ocs_compatible', '1');
          const resp = await fetch({json.dumps(f"{store_base}/p/{product_id}/updatepploadfile/")}, {{
            method: 'POST',
            body: form,
            credentials: 'include',
            headers: {{
              'Accept': 'application/json',
              'Origin': {json.dumps(store_base)},
              'Referer': {json.dumps(f"{store_base}/p/{product_id}/")},
            }},
          }});
          const text = await resp.text();
          let json = null;
          try {{ json = JSON.parse(text); }} catch (e) {{}}
          return {{ status: resp.status, ok: !!(json && json.status === 'ok') }};
        }})()""",
        await_promise=True,
        timeout=60,
    )
    if not update or not update.get("ok"):
        die("file_metadata_update_failed")
    status(f"upload:file_version={version}")

    # Product Basics version + Save
    await eval_js(
        ws,
        mid,
        'document.querySelector(\'a[href*="add-product-form-h-0"]\')?.click(); true',
    )
    await asyncio.sleep(0.5)
    await eval_js(
        ws,
        mid,
        f"""(() => {{
          const v = document.querySelector('#version, input[name=version]');
          if (!v) return false;
          v.value = {json.dumps(version)};
          if (window.jQuery) jQuery(v).val({json.dumps(version)}).trigger('change').trigger('input');
          v.dispatchEvent(new Event('input', {{bubbles: true}}));
          v.dispatchEvent(new Event('change', {{bubbles: true}}));
          return true;
        }})()""",
    )
    await eval_js(
        ws,
        mid,
        """(() => {
          const save = [...document.querySelectorAll('a,button,input')].find(b => {
            const t = (b.innerText || b.value || '').trim();
            const h = b.getAttribute('href') || '';
            return t === 'Save' || t === 'Finish' || h.includes('#finish');
          });
          if (save) { save.click(); return true; }
          return false;
        })()""",
    )
    await asyncio.sleep(3)
    status(f"upload:product_version={version}")
    return file_id


async def run(archive: pathlib.Path, version: str, cookie_file: pathlib.Path, product_id: str, store_base: str) -> None:
    try:
        import websockets
    except ImportError:
        die("websockets_missing")

    cookies = parse_netscape(cookie_file)
    if not cookies:
        die("empty_cookie_file")

    chrome = shutil.which("google-chrome") or shutil.which("google-chrome-stable")
    if not chrome:
        die("chrome_missing")

    port = 9222
    user_data = pathlib.Path(tempfile.mkdtemp(prefix="codexbar-kde-cdp."))
    proc = None
    try:
        proc = subprocess.Popen(
            [
                chrome,
                f"--user-data-dir={user_data}",
                f"--remote-debugging-port={port}",
                "--no-first-run",
                "--no-default-browser-check",
                "about:blank",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        wait_for_cdp(port)
        page = pick_page(port)
        mid = [0]
        async with websockets.connect(page["webSocketDebuggerUrl"], max_size=80_000_000) as ws:
            await cdp_call(ws, mid, "Network.enable")
            await cdp_call(ws, mid, "Page.enable")
            await cdp_call(ws, mid, "Runtime.enable")
            await cdp_call(ws, mid, "DOM.enable")
            set_ok = 0
            for cookie in cookies:
                # Do not log cookie names/values.
                res = await cdp_call(ws, mid, "Network.setCookie", cookie)
                if res.get("result", {}).get("success"):
                    set_ok += 1
            if set_ok == 0:
                die("cookie_inject_failed")
            status(f"auth:cookies_injected count={set_ok}")
            # Optional GitHub cookies from main profile are not required; OAuth UI can complete with a signed-in main browser.
            # For GitHub select_account, inject github cookies if the cookie file has them (auth.py includes opendesktop+kde only).
            file_id = await upload_via_files_ui(ws, mid, archive, version, product_id, store_base)
        status(f"ok file_id={file_id}")
    finally:
        if proc is not None and proc.poll() is None:
            try:
                proc.terminate()
                proc.wait(timeout=5)
            except Exception:
                try:
                    proc.kill()
                except Exception:
                    pass
        shutil.rmtree(user_data, ignore_errors=True)


def main(argv: list[str]) -> None:
    if len(argv) < 4:
        sys.stderr.write(
            "Usage: kde-store-browser-upload.py <archive.plasmoid> <version> <cookie-file> "
            "[product_id] [store_base]\n"
        )
        raise SystemExit(2)
    archive = pathlib.Path(argv[1]).resolve()
    version = argv[2]
    cookie_file = pathlib.Path(argv[3])
    product_id = argv[4] if len(argv) > 4 else PRODUCT_ID_DEFAULT
    store_base = argv[5] if len(argv) > 5 else STORE_BASE_DEFAULT
    if not archive.is_file():
        die("archive_missing")
    if not cookie_file.is_file():
        die("cookie_file_missing")
    asyncio.run(run(archive, version, cookie_file, product_id, store_base))


if __name__ == "__main__":
    main(sys.argv)
