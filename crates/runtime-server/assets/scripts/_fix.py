import re, os

# Fix bare {{PLACEHOLDER}} in JSON arrays: {{X}} → "{{X}}"
# Only replace when {{ is preceded by [ or , or space+TAB (array context)
pattern = re.compile(r'(\[|, )(\{\{[^}]+\}\})')
fixed = 0
for f in os.listdir("templates"):
    if not f.endswith(".json"):
        continue
    path = f"templates/{f}"
    with open(path, "r", encoding="utf-8") as fh:
        text = fh.read()
    new_text, n = pattern.subn(r'\1"\2"', text)
    if n:
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(new_text)
        print(f"  Fixed {n} placeholders in {f}")
        fixed += n

print(f"\nTotal: {fixed} placeholders fixed in all templates")
