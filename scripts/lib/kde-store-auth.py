#!/usr/bin/env python3
"""Discover a store.kde.org session from local Chrome into a cookie *file*.

Privacy contract (non-negotiable):
  - Never print cookie values, token substrings, or decrypted secrets.
  - Only write cookies into the caller-provided file path (mode 0600).
  - Stdout is a status line only: "ok" or an error reason with no secret material.
  - Exit 0 on success, 1 on failure.

Usage:
  kde-store-auth.py write-cookie-file /path/to/cookies.txt
  kde-store-auth.py check-edit [/path/to/cookies.txt]
"""

from __future__ import annotations

import hashlib
import pathlib
import shutil
import sqlite3
import sys
import tempfile
import urllib.error
import urllib.request

PRODUCT_ID = "2365275"
STORE_BASE = "https://store.kde.org"
EDIT_URL = f"{STORE_BASE}/p/{PRODUCT_ID}/edit/"


def _die(message: str, code: int = 1) -> None:
    # Status-only: never include secret material in message.
    sys.stderr.write(f"{message}\n")
    raise SystemExit(code)


def chrome_profiles(home: pathlib.Path) -> list[pathlib.Path]:
    user_data = home / ".config" / "google-chrome"
    return [user_data / "Default", *sorted(user_data.glob("Profile *"))]


def chrome_safe_storage_passwords() -> list[bytes]:
    passwords: list[bytes] = []
    try:
        import secretstorage
    except Exception:
        return passwords
    try:
        bus = secretstorage.dbus_init()
        for collection in secretstorage.get_all_collections(bus):
            try:
                items = collection.get_all_items()
            except Exception:
                continue
            for item in items:
                try:
                    label = item.get_label() or ""
                except Exception:
                    continue
                if "Chrome" not in label and "chrome" not in label:
                    continue
                try:
                    secret = bytes(item.get_secret())
                except Exception:
                    continue
                if secret and secret not in passwords:
                    passwords.append(secret)
    except Exception:
        return passwords
    return passwords


def decrypt_chrome_value(encrypted: bytes, key: bytes) -> str | None:
    if not encrypted:
        return None
    prefix = encrypted[:3]
    if prefix not in (b"v10", b"v11"):
        try:
            return encrypted.decode("utf-8")
        except UnicodeDecodeError:
            return None
    try:
        from Cryptodome.Cipher import AES
    except Exception:
        return None
    data = encrypted[3:]
    try:
        decrypted = AES.new(key, AES.MODE_CBC, b" " * 16).decrypt(data)
    except Exception:
        return None
    if not decrypted:
        return None
    pad = decrypted[-1]
    if 1 <= pad <= 16 and decrypted.endswith(bytes([pad]) * pad):
        decrypted = decrypted[:-pad]
    candidates: list[bytes] = []
    if len(decrypted) > 32:
        candidates.append(decrypted[32:])
    candidates.append(decrypted)
    if len(decrypted) > 32:
        candidates.append(decrypted[:-32])
    for candidate in candidates:
        try:
            text = candidate.decode("utf-8")
        except UnicodeDecodeError:
            continue
        if text:
            return text
    return None


def try_browser_cookie3(database: pathlib.Path) -> list[tuple[str, str, str, str, bool]] | None:
    """Return list of (domain, path, name, value, secure) or None."""
    try:
        import browser_cookie3
    except Exception:
        return None
    try:
        jar = browser_cookie3.chrome(cookie_file=str(database), domain_name="kde.org")
    except Exception:
        return None
    rows: list[tuple[str, str, str, str, bool]] = []
    for cookie in jar:
        if not cookie.value:
            continue
        domain = cookie.domain or ""
        path = cookie.path or "/"
        secure = bool(getattr(cookie, "secure", False))
        rows.append((domain, path, cookie.name, cookie.value, secure))
    if not any(name == "__ocs_id" for _d, _p, name, _v, _s in rows):
        return None
    return rows


def try_keyring_decrypt(database: pathlib.Path) -> list[tuple[str, str, str, str, bool]] | None:
    passwords = chrome_safe_storage_passwords()
    if not passwords:
        return None
    tmp_dir = pathlib.Path(tempfile.mkdtemp(prefix="codexbar-chrome-cookies."))
    try:
        copy = tmp_dir / "Cookies"
        shutil.copy2(database, copy)
        conn = sqlite3.connect(f"file:{copy}?mode=ro", uri=True)
        try:
            # kde.org + opendesktop hosts used by the store OAuth bounce.
            # Include GitHub session cookies so isolated-Chrome OAuth can
            # complete "Continue as …" without a password prompt. Values stay
            # only in the cookie file (never printed).
            rows = conn.execute(
                """
                SELECT host_key, name, value, encrypted_value, path, is_secure
                FROM cookies
                WHERE host_key LIKE '%kde.org%'
                   OR host_key LIKE '%opendesktop.org%'
                   OR host_key LIKE '%github.com%'
                """
            ).fetchall()
        finally:
            conn.close()
        for password in passwords:
            key = hashlib.pbkdf2_hmac("sha1", password, b"saltysalt", 1, dklen=16)
            out: list[tuple[str, str, str, str, bool]] = []
            for host, name, value, encrypted, path, is_secure in rows:
                text = value or ""
                if not text and encrypted is not None:
                    text = decrypt_chrome_value(bytes(encrypted), key) or ""
                if text:
                    out.append((host, path or "/", name, text, bool(is_secure)))
            if any(name == "__ocs_id" for _h, _p, name, _v, _s in out):
                return out
    except Exception:
        return None
    finally:
        shutil.rmtree(tmp_dir, ignore_errors=True)
    return None


def score_session(rows: list[tuple[str, str, str, str, bool]]) -> tuple[int, int, int]:
    names = {name for _d, _p, name, _v, _s in rows}
    return (
        int("remember_token" in names),
        int("verified" in names),
        len(rows),
    )


def discover_cookie_rows(home: pathlib.Path) -> list[tuple[str, str, str, str, bool]] | None:
    candidates: list[list[tuple[str, str, str, str, bool]]] = []
    for profile in chrome_profiles(home):
        database = profile / "Cookies"
        if not database.is_file():
            continue
        for loader in (try_browser_cookie3, try_keyring_decrypt):
            rows = loader(database)
            if rows:
                candidates.append(rows)
    if not candidates:
        return None
    candidates.sort(key=score_session, reverse=True)
    return candidates[0]


def write_netscape_cookie_file(
    path: pathlib.Path, rows: list[tuple[str, str, str, str, bool]]
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = ["# Netscape HTTP Cookie File", "# codexbar-plasmoid release auth — do not commit", ""]
    # Far-future expiry; session cookies still work for curl when sent.
    exp = 2000000000
    for domain, path_value, name, value, secure in rows:
        # Strip control chars so the file stays parseable; never log value.
        value = value.replace("\t", "").replace("\n", "").replace("\r", "")
        include_sub = "TRUE" if domain.startswith(".") else "FALSE"
        secure_flag = "TRUE" if secure else "FALSE"
        lines.append(
            f"{domain}\t{include_sub}\t{path_value or '/'}\t{secure_flag}\t{exp}\t{name}\t{value}"
        )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    path.chmod(0o600)


def cookie_header_from_rows(rows: list[tuple[str, str, str, str, bool]]) -> str:
    # Internal helper only — callers must not print this.
    return "; ".join(f"{name}={value}" for _d, _p, name, value, _s in rows if value)


def edit_page_authorized(cookie_file: pathlib.Path) -> bool:
    """Return True if the cookie file can open the product edit backend."""
    try:
        req = urllib.request.Request(
            EDIT_URL,
            headers={
                "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/150 Safari/537.36",
                "Accept": "text/html",
            },
            method="GET",
        )
        # Build Cookie header without printing it.
        rows: list[tuple[str, str, str, str, bool]] = []
        for line in cookie_file.read_text(encoding="utf-8").splitlines():
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 7:
                continue
            domain, _inc, path, secure, _exp, name, value = parts[:7]
            rows.append((domain, path, name, value, secure.upper() == "TRUE"))
        if not rows:
            return False
        req.add_header("Cookie", cookie_header_from_rows(rows))
        with urllib.request.urlopen(req, timeout=30) as resp:
            final = resp.geturl()
            body = resp.read(200_000).decode("utf-8", errors="replace")
        if "/login" in final or "opendesktop.org/login" in final:
            return False
        return "welcome to your store backend" in body.lower()
    except (urllib.error.URLError, urllib.error.HTTPError, OSError, TimeoutError):
        return False


def cmd_write_cookie_file(path: pathlib.Path) -> None:
    home = pathlib.Path.home()
    rows = discover_cookie_rows(home)
    if not rows:
        _die("no_chrome_store_session")
    write_netscape_cookie_file(path, rows)
    # Never print cookie contents — only a machine-readable status.
    print("ok")


def cmd_check_edit(path: pathlib.Path | None) -> None:
    if path is None:
        tmp = pathlib.Path(tempfile.mkstemp(prefix="codexbar-kde-auth.", suffix=".cookies")[1])
        try:
            cmd_write_cookie_file(tmp)
            path = tmp
            authorized = edit_page_authorized(path)
        finally:
            try:
                tmp.unlink(missing_ok=True)
            except OSError:
                pass
    else:
        if not path.is_file():
            _die("cookie_file_missing")
        authorized = edit_page_authorized(path)
    print("authorized" if authorized else "unauthorized")
    raise SystemExit(0 if authorized else 1)


def main(argv: list[str]) -> None:
    if len(argv) < 2 or argv[1] in {"-h", "--help"}:
        sys.stderr.write(
            "Usage:\n"
            "  kde-store-auth.py write-cookie-file <path>\n"
            "  kde-store-auth.py check-edit [cookie-file]\n"
        )
        raise SystemExit(2)
    action = argv[1]
    if action == "write-cookie-file":
        if len(argv) != 3:
            _die("usage: write-cookie-file <path>", 2)
        cmd_write_cookie_file(pathlib.Path(argv[2]))
        return
    if action == "check-edit":
        cookie_path = pathlib.Path(argv[2]) if len(argv) >= 3 else None
        cmd_check_edit(cookie_path)
        return
    _die(f"unknown_action:{action}", 2)


if __name__ == "__main__":
    main(sys.argv)
