import json
import sqlite3

JSON_PATH = "titles.json"
DB_PATH = "titles.db"

con = sqlite3.connect(DB_PATH)
cur = con.cursor()

cur.execute("""
CREATE TABLE IF NOT EXISTS titles (
    title_id TEXT PRIMARY KEY,
    name     TEXT,
    iconUrl  TEXT,
    isDemo   INTEGER
)
""")

con.execute("PRAGMA journal_mode=WAL")
con.execute("PRAGMA synchronous=OFF")
con.execute("PRAGMA cache_size=-1048576")

print("Loading JSON...")
with open(JSON_PATH, "r", encoding="utf-8") as f:
    data = json.load(f)

print(f"Loaded {len(data)} titles. Inserting...")

rows = []
BATCH = 50000

for i, (tid, obj) in enumerate(data.items(), 1):
    rows.append((
        tid,
        obj.get("name"),
        obj.get("iconUrl"),
        1 if obj.get("isDemo") is True else (0 if obj.get("isDemo") is False else None),
    ))

    if i % BATCH == 0:
        cur.executemany("INSERT OR REPLACE INTO titles VALUES (?,?,?,?)", rows)
        con.commit()
        print(f"  {i:,} / {len(data):,}")
        rows.clear()

if rows:
    cur.executemany("INSERT OR REPLACE INTO titles VALUES (?,?,?,?)", rows)
    con.commit()

con.execute("PRAGMA journal_mode=DELETE")
con.execute("PRAGMA synchronous=NORMAL")

con.commit()
con.close()
print(f"Done. Database at {DB_PATH}")
