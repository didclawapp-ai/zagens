import json, os, sys
errors = []
for f in os.listdir("templates"):
    if f.endswith(".json"):
        try:
            with open(f"templates/{f}", "r", encoding="utf-8") as fh:
                json.load(fh)
            print(f"  OK  {f}")
        except Exception as e:
            errors.append(f)
            print(f"FAIL  {f}: {e}")
if errors:
    print(f"\n{len(errors)} template(s) failed")
    sys.exit(1)
else:
    print(f"\nAll templates valid")
