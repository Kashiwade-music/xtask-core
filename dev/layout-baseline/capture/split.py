"""Split captured ELK request/response pairs into one JSON file per diagram."""
import glob
import json
import re
import sys
from pathlib import Path

source_dir, out_dir = Path(sys.argv[1]), Path(sys.argv[2])
out_dir.mkdir(parents=True, exist_ok=True)
seen = {}
for path in sorted(glob.glob(str(source_dir / "*.jsonl"))):
    for line in open(path, encoding="utf-8"):
        pair = json.loads(line)
        if pair["response"].get("error"):
            raise SystemExit(f"layout error in capture: {pair['request']['title']}")
        title = pair["request"]["title"]
        m = re.match(r"^(.+?)::(.+?) \((.+?), (.+?)\): module dependencies$", title)
        if m:
            name = ".".join(m.groups()).replace("+", "_")
        elif title.startswith("Workspace crate dependencies"):
            name = "crates"
        else:
            raise SystemExit(f"unexpected title {title!r}")
        if name in seen:
            raise SystemExit(f"duplicate diagram name {name}")
        seen[name] = title
        record = {"input": pair["request"], "scene": pair["response"]["scene"]}
        text = json.dumps(record, ensure_ascii=False, separators=(",", ":")) + "\n"
        (out_dir / f"{name}.json").write_text(text, encoding="utf-8", newline="\n")
print(len(seen), "diagrams written to", out_dir)
