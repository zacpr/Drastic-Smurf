#!/usr/bin/env python3
"""Generate assets/api_examples.json from Elastic's published OpenAPI specs.

The Console presets (src/modules/console.rs) are matched to the operations in
the Elasticsearch and Kibana OpenAPI documents. Each operation's online API
reference URL and its request/response examples are written out keyed by
"METHOD /preset/path" (the preset path without its query string).

Usage (from the repo root):

    curl -sSfL -o /tmp/es.json \
      https://raw.githubusercontent.com/elastic/elasticsearch-specification/main/output/openapi/elasticsearch-openapi.json
    curl -sSfL -o /tmp/kibana.yaml \
      https://raw.githubusercontent.com/elastic/kibana/main/oas_docs/output/kibana.yaml
    python3 scripts/gen_api_examples.py /tmp/es.json /tmp/kibana.yaml

Requires PyYAML. The public HTML API reference at elastic.co/docs/api is
rendered from these same specs, so it holds no extra examples.
"""

import json
import re
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
CONSOLE_RS = ROOT / "src" / "modules" / "console.rs"
OUTPUT = ROOT / "assets" / "api_examples.json"

# Keep the embedded file small: a handful of examples per operation, and no
# response example so long it would bury the response box.
MAX_EXAMPLES = 4
MAX_EXAMPLE_CHARS = 6000
DOC_BASE = {
    "es": "https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-",
    "kibana": "https://www.elastic.co/docs/api/doc/kibana/operation/operation-",
}


def doc_url(flavour, operation):
    op_id = operation.get("operationId")
    if not op_id:
        return None
    if flavour == "es":
        # Path variants of one ES API are numbered (search-1, search-3, ...);
        # the docs page lives at the bare name and the numbered ones redirect.
        op_id = re.sub(r"-\d+$", "", op_id)
    # The docs site serves operation pages under lowercased ids (getAlertingHealth ->
    # operation-getalertinghealth); the camelCase spelling 404s.
    return DOC_BASE[flavour] + op_id.lower()


def load_presets():
    source = CONSOLE_RS.read_text()
    start = source.index("const PRESETS")
    block = source[start : source.index("\n];", start)]
    presets = []
    for match in re.finditer(r"ConsolePreset \{(.*?)\n    \},", block, re.S):
        body = match.group(1)

        def field(name):
            found = re.search(name + r': "(.*?)",', body)
            return found.group(1) if found else None

        presets.append(
            {
                "name": field("name"),
                "method": field("method"),
                "path": field("path"),
                "kibana": "use_kibana: true" in body,
            }
        )
    return presets


def preset_key(method, path):
    return f"{method} {path.split('?', 1)[0]}"


def segments(path):
    parts = [s for s in path.split("?", 1)[0].strip("/").split("/") if s != ""]
    # Kibana space prefix (/s/{{space_id}}/api/...): the specs document the bare path.
    if len(parts) > 2 and parts[0] == "s" and parts[2] == "api":
        parts = parts[2:]
    return parts


def match_penalty(preset_path, spec_path):
    """Lower is better; None means the paths don't match."""
    preset, spec = segments(preset_path), segments(spec_path)
    if len(preset) != len(spec):
        return None
    penalty = 0
    for ours, theirs in zip(preset, spec):
        ours_param = ours.startswith("{{") and ours.endswith("}}")
        theirs_param = theirs.startswith("{") and theirs.endswith("}")
        if ours_param:
            if not theirs_param:
                return None
        elif theirs_param:
            penalty += 1  # a literal value (e.g. "my-dashboard-id") in a param slot
        elif ours != theirs:
            return None
    return penalty


def resolve(spec, node):
    seen = 0
    while isinstance(node, dict) and "$ref" in node and seen < 10:
        target = spec
        for part in node["$ref"].lstrip("#/").split("/"):
            target = target[part]
        node = target
        seen += 1
    return node


def render_value(value):
    if isinstance(value, str):
        text = value.strip()
        try:
            json.loads(text)
            return text  # keep the docs' own (often compact) layout
        except ValueError:
            return text
    return json.dumps(value, indent=2, ensure_ascii=False)


def collect_examples(spec, content):
    content = resolve(spec, content) or {}
    media = content.get("application/json") or next(iter(content.values()), None)
    media = resolve(spec, media) or {}
    out = []
    examples = media.get("examples") or {}
    for key, example in examples.items():
        example = resolve(spec, example)
        if not isinstance(example, dict) or "value" not in example:
            continue
        summary = (example.get("summary") or example.get("description") or key).strip()
        summary = " ".join(summary.split())
        if len(summary) > 90:
            summary = summary[:87].rstrip() + "..."
        value = render_value(example["value"])
        if value and len(value) <= MAX_EXAMPLE_CHARS:
            out.append({"summary": summary, "value": value})
    if not out and "example" in media:
        value = render_value(media["example"])
        if value and len(value) <= MAX_EXAMPLE_CHARS:
            out.append({"summary": "Example", "value": value})
    return out[:MAX_EXAMPLES]


def operation_examples(spec, operation):
    request = []
    body = resolve(spec, operation.get("requestBody"))
    if isinstance(body, dict) and body.get("content"):
        request = collect_examples(spec, body["content"])
    response = []
    responses = operation.get("responses") or {}
    for code in sorted(responses, key=str):
        if not str(code).startswith("2"):
            continue
        resp = resolve(spec, responses[code])
        if isinstance(resp, dict) and resp.get("content"):
            response = collect_examples(spec, resp["content"])
        if response:
            break
    return request, response


def find_operation(spec, method, path):
    best = None
    for spec_path, item in spec.get("paths", {}).items():
        operation = item.get(method.lower()) if isinstance(item, dict) else None
        if not isinstance(operation, dict):
            continue
        penalty = match_penalty(path, spec_path)
        if penalty is not None and (best is None or penalty < best[0]):
            best = (penalty, spec_path, operation)
    return best


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    es = json.loads(Path(sys.argv[1]).read_text())
    kibana = yaml.load(Path(sys.argv[2]).read_text(), Loader=getattr(yaml, "CSafeLoader", yaml.SafeLoader))

    result = {}
    for preset in load_presets():
        flavour = "kibana" if preset["kibana"] else "es"
        spec = kibana if preset["kibana"] else es
        found = find_operation(spec, preset["method"], preset["path"])
        if not found:
            print(f"  no spec operation: {preset['method']} {preset['path']} ({preset['name']})")
            continue
        _, spec_path, operation = found
        request, response = operation_examples(spec, operation)
        key = preset_key(preset["method"], preset["path"])
        entry = result.setdefault(key, {"operation": f"{preset['method']} {spec_path}"})
        url = doc_url(flavour, operation)
        if url:
            entry["doc_url"] = url
        if request:
            entry["request"] = request
        if response:
            entry["response"] = response

    OUTPUT.write_text(json.dumps(result, indent=1, ensure_ascii=False, sort_keys=True) + "\n")
    links = sum(1 for e in result.values() if "doc_url" in e)
    requests = sum(1 for e in result.values() if "request" in e)
    responses = sum(1 for e in result.values() if "response" in e)
    print(
        f"wrote {OUTPUT.relative_to(ROOT)}: {len(result)} operations, {links} doc links, "
        f"{requests} with request examples, {responses} with response examples, "
        f"{OUTPUT.stat().st_size // 1024} KiB"
    )


if __name__ == "__main__":
    main()
