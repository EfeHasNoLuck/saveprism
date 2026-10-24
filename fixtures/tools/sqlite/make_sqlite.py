"""Writes the synthetic SQLite save used by the SQLite codec tests.

Invented data only: no game code, no real player data. Python 3 standard library.
Run from the repository root:

    python fixtures/tools/sqlite/make_sqlite.py
"""

from pathlib import Path
import sqlite3

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "synthetic" / "sqlite" / "game.sqlite"


def main() -> None:
    OUT.parent.mkdir(parents=True, exist_ok=True)
    if OUT.exists():
        OUT.unlink()
    connection = sqlite3.connect(OUT)
    try:
        connection.executescript(
            """
            PRAGMA journal_mode = DELETE;
            PRAGMA user_version = 7;

            CREATE TABLE player (
                id INTEGER PRIMARY KEY,
                gold INTEGER NOT NULL,
                hp REAL NOT NULL,
                name TEXT NOT NULL,
                flags BLOB,
                nickname TEXT
            );
            INSERT INTO player VALUES
                (1, 1250, 87.5, 'Aria', X'00FF10', NULL),
                (2, 40, 12.25, 'Bram', X'', 'B');

            CREATE TABLE inventory (
                item TEXT NOT NULL,
                count INTEGER NOT NULL
            );
            INSERT INTO inventory VALUES
                ('potion', 3),
                ('ether', 1),
                ('rope', 10);

            CREATE TABLE variables (
                key TEXT PRIMARY KEY,
                value
            ) WITHOUT ROWID;
            INSERT INTO variables VALUES
                ('chapter', 4),
                ('difficulty', 'normal'),
                ('music', 0.75);

            CREATE TABLE generated (
                amount INTEGER NOT NULL,
                doubled INTEGER GENERATED ALWAYS AS (amount * 2) VIRTUAL
            );
            INSERT INTO generated (amount) VALUES (21);

            CREATE VIEW player_summary AS
                SELECT name, gold FROM player;
            """
        )
        connection.commit()
        connection.execute("VACUUM")
    finally:
        connection.close()


if __name__ == "__main__":
    main()
