## Why

The current output embeds a policy decision — labelling one file `keep` and the rest `dupe` — that the tool cannot make correctly: it has no knowledge of which location the user considers authoritative. Removing this policy makes the output purely factual and leaves the decision to the caller (GUI, script, or the user directly).

## What Changes

- **BREAKING**: Remove `keep` and `dupe` line prefixes. Each duplicate group lists paths directly, one per line, with no label indicating which to delete.
- **BREAKING**: Update summary line on `stderr`: replace "N duplicate file(s) found, wasting X MB" with "N duplicate group(s) found, X MB of duplicated content" to reflect that groups, not individual files, are the unit of output.
- The `link:` prefix (introduced in the inode-aliasing change) is retained unchanged.
- No changes to the `# <bytes>` group header or to stream assignment (stdout / stderr split).

## Capabilities

### New Capabilities

_(none)_

### Modified Capabilities

- `output-format`: Remove `keep`/`dupe` labels from duplicate group lines; update summary wording.

## Impact

- `main.rs`: Output loop simplified — no index tracking, no label selection.
- `openspec/specs/output-format/spec.md`: Requirements around `keep`/`dupe` labels and summary wording updated.
- Any downstream parser that consumes `keep`/`dupe` tokens will break; callers must be updated to treat every non-header, non-`link:` line as a member of the group.
