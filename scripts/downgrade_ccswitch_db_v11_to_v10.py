#!/usr/bin/env python3
"""
Downgrade a CC Switch SQLite database from schema v11 to v10 without losing
normal application data.

What v11 changed:
- Added `enabled_codefree_o` to `mcp_servers`

What this script does:
1. Finds the database (or uses --db)
2. Creates a full SQLite backup beside the original DB
3. Exports all `enabled_codefree_o` values to a JSON sidecar file
4. Rebuilds `mcp_servers` back to the v10 shape
5. Sets `PRAGMA user_version = 10`
6. Runs `PRAGMA integrity_check`

Important:
- Close all CC Switch instances first, including dev builds.
- If your dev build keeps using the same DB after this, it will upgrade it back
  to v11 on next launch.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import shutil
import sqlite3
import sys
from pathlib import Path
from typing import Iterable


EXPECTED_V11_COLUMNS = [
    "id",
    "name",
    "server_config",
    "description",
    "homepage",
    "docs",
    "tags",
    "enabled_claude",
    "enabled_codex",
    "enabled_gemini",
    "enabled_opencode",
    "enabled_codefree_o",
    "enabled_hermes",
]

V10_CREATE_MCP_SERVERS_SQL = """
CREATE TABLE mcp_servers__v10 (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    server_config TEXT NOT NULL,
    description TEXT,
    homepage TEXT,
    docs TEXT,
    tags TEXT NOT NULL DEFAULT '[]',
    enabled_claude BOOLEAN NOT NULL DEFAULT 0,
    enabled_codex BOOLEAN NOT NULL DEFAULT 0,
    enabled_gemini BOOLEAN NOT NULL DEFAULT 0,
    enabled_opencode BOOLEAN NOT NULL DEFAULT 0,
    enabled_hermes BOOLEAN NOT NULL DEFAULT 0
)
""".strip()

V10_COPY_SQL = """
INSERT INTO mcp_servers__v10 (
    id,
    name,
    server_config,
    description,
    homepage,
    docs,
    tags,
    enabled_claude,
    enabled_codex,
    enabled_gemini,
    enabled_opencode,
    enabled_hermes
)
SELECT
    id,
    name,
    server_config,
    description,
    homepage,
    docs,
    tags,
    enabled_claude,
    enabled_codex,
    enabled_gemini,
    enabled_opencode,
    enabled_hermes
FROM mcp_servers
""".strip()


def now_stamp() -> str:
    return dt.datetime.now().strftime("%Y%m%d-%H%M%S")


def default_candidates() -> list[Path]:
    candidates: list[Path] = []

    def add(base: str | None) -> None:
        if not base:
            return
        path = Path(base).expanduser().resolve() / ".cc-switch" / "cc-switch.db"
        if path not in candidates:
            candidates.append(path)

    add(os.environ.get("USERPROFILE"))
    add(os.environ.get("HOME"))
    return [path for path in candidates if path.exists()]


def choose_db(explicit: str | None) -> Path:
    if explicit:
        db_path = Path(explicit).expanduser().resolve()
        if not db_path.exists():
            raise SystemExit(f"Database does not exist: {db_path}")
        return db_path

    candidates = default_candidates()
    if not candidates:
        raise SystemExit(
            "Could not auto-detect cc-switch.db.\n"
            "Pass it explicitly with --db, for example:\n"
            r'  python scripts\downgrade_ccswitch_db_v11_to_v10.py --db "%USERPROFILE%\.cc-switch\cc-switch.db"'
        )
    if len(candidates) > 1:
        joined = "\n".join(f"  - {path}" for path in candidates)
        raise SystemExit(
            "Found multiple candidate databases. Pass --db explicitly:\n" + joined
        )
    return candidates[0]


def get_user_version(conn: sqlite3.Connection) -> int:
    row = conn.execute("PRAGMA user_version").fetchone()
    if row is None:
        raise RuntimeError("Failed to read PRAGMA user_version")
    return int(row[0])


def get_table_columns(conn: sqlite3.Connection, table: str) -> list[str]:
    return [row[1] for row in conn.execute(f"PRAGMA table_info({table})")]


def backup_sqlite(db_path: Path, backup_path: Path) -> None:
    src = sqlite3.connect(str(db_path))
    dst = sqlite3.connect(str(backup_path))
    try:
        src.backup(dst)
    finally:
        dst.close()
        src.close()


def export_codefree_o_sidecar(
    conn: sqlite3.Connection, db_path: Path, sidecar_path: Path
) -> int:
    rows = conn.execute(
        """
        SELECT id, enabled_codefree_o
        FROM mcp_servers
        WHERE COALESCE(enabled_codefree_o, 0) <> 0
        ORDER BY id
        """
    ).fetchall()

    payload = {
        "source_db": str(db_path),
        "exported_at": dt.datetime.now().isoformat(timespec="seconds"),
        "schema_from": 11,
        "schema_to": 10,
        "preserved_rows": [
            {"id": row[0], "enabled_codefree_o": int(row[1])} for row in rows
        ],
    }
    sidecar_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
    return len(rows)


def integrity_check(conn: sqlite3.Connection) -> str:
    row = conn.execute("PRAGMA integrity_check").fetchone()
    if row is None:
        raise RuntimeError("PRAGMA integrity_check returned no rows")
    return str(row[0])


def ensure_expected_shape(conn: sqlite3.Connection) -> None:
    columns = get_table_columns(conn, "mcp_servers")
    missing = [name for name in EXPECTED_V11_COLUMNS if name not in columns]
    if missing:
        raise RuntimeError(
            "mcp_servers does not look like schema v11. Missing columns: "
            + ", ".join(missing)
        )


def downgrade(db_path: Path, dry_run: bool) -> None:
    stamp = now_stamp()
    backup_path = db_path.with_name(f"{db_path.stem}.pre-v10-downgrade.{stamp}.bak")
    sidecar_path = db_path.with_name(
        f"{db_path.stem}.codefree-o-preserved.{stamp}.json"
    )

    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row
    try:
        version = get_user_version(conn)
        if version < 11:
            raise SystemExit(
                f"Database is already at user_version={version}; no downgrade needed."
            )
        if version > 11:
            raise SystemExit(
                f"Database is at user_version={version}; this script only handles v11 -> v10."
            )

        ensure_expected_shape(conn)

        if dry_run:
            preserved = conn.execute(
                "SELECT COUNT(*) FROM mcp_servers WHERE COALESCE(enabled_codefree_o, 0) <> 0"
            ).fetchone()[0]
            print(f"[dry-run] database: {db_path}")
            print(f"[dry-run] user_version: {version}")
            print(f"[dry-run] would write backup: {backup_path}")
            print(f"[dry-run] would write sidecar: {sidecar_path}")
            print(f"[dry-run] would preserve {preserved} codefree-o MCP flags")
            return

        print(f"Using database: {db_path}")
        print(f"Creating backup: {backup_path}")
        backup_sqlite(db_path, backup_path)

        preserved_rows = export_codefree_o_sidecar(conn, db_path, sidecar_path)
        print(f"Exported codefree-o MCP flags: {sidecar_path} ({preserved_rows} rows)")

        conn.execute("PRAGMA foreign_keys = OFF")
        conn.execute("BEGIN IMMEDIATE")
        try:
            conn.execute("DROP TABLE IF EXISTS mcp_servers__v10")
            conn.execute(V10_CREATE_MCP_SERVERS_SQL)
            conn.execute(V10_COPY_SQL)
            conn.execute("DROP TABLE mcp_servers")
            conn.execute("ALTER TABLE mcp_servers__v10 RENAME TO mcp_servers")
            conn.execute("PRAGMA user_version = 10")
            conn.commit()
        except Exception:
            conn.rollback()
            raise
        finally:
            conn.execute("PRAGMA foreign_keys = ON")

        new_version = get_user_version(conn)
        if new_version != 10:
            raise RuntimeError(f"Expected user_version=10, got {new_version}")

        columns = get_table_columns(conn, "mcp_servers")
        if "enabled_codefree_o" in columns:
            raise RuntimeError("Downgrade failed: enabled_codefree_o column still exists")

        check = integrity_check(conn)
        if check.lower() != "ok":
            raise RuntimeError(f"SQLite integrity_check failed: {check}")

        print("Downgrade complete.")
        print(f"New user_version: {new_version}")
        print("Integrity check: ok")
        print(f"Full DB backup: {backup_path}")
        print(f"Preserved codefree-o sidecar: {sidecar_path}")
    finally:
        conn.close()


def parse_args(argv: Iterable[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Downgrade CC Switch SQLite DB from schema v11 to v10."
    )
    parser.add_argument(
        "--db",
        help="Path to cc-switch.db. If omitted, the script auto-detects common Windows paths.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Inspect only. Do not modify the database.",
    )
    return parser.parse_args(list(argv))


def main(argv: Iterable[str]) -> int:
    args = parse_args(argv)
    db_path = choose_db(args.db)
    downgrade(db_path, args.dry_run)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except sqlite3.OperationalError as exc:
        raise SystemExit(
            "SQLite operation failed. Make sure all CC Switch instances are closed.\n"
            f"Details: {exc}"
        )
