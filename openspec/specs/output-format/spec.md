# Spec: Output Format

## Overview

`dedup` separates its output into two streams: machine-readable results on `stdout`
and human-readable progress on `stderr`. This separation allows `stdout` to be piped
or parsed reliably without noise from progress messages.

## Requirements

### Stream Assignment

- All duplicate group output MUST be written to `stdout`.
- All progress messages, warnings, and the final summary MUST be written to `stderr`.
- No diagnostic or progress content MUST appear on `stdout`.

### Duplicate Group Format

Each duplicate group SHALL begin with a header line of the form:

```
# <N> bytes
```

where `<N>` is the exact byte size shared by all files in the group.

Within each group, each physical file (AliasGroup representative) SHALL be printed as an indented path line:

```
  <path>
```

Hard-linked aliases of that physical file SHALL immediately follow, each on its own line prefixed with `  link: `:

```
  link: <path>
```

No labels indicating which file to keep or delete SHALL appear in the output. The order in which group members are printed is unspecified; callers MUST NOT rely on any particular ordering within a group.

#### Scenario: Group with two independent duplicate files
- **WHEN** two files with different inodes have identical content
- **THEN** stdout contains a `# <N> bytes` header followed by two indented path lines with no `keep` or `dupe` prefix

#### Scenario: Group where one physical file has hard-linked aliases
- **WHEN** a content-duplicate group contains an AliasGroup with two aliases
- **THEN** the representative path is printed as `  <path>` and the alias is printed as `  link: <path>` immediately after

#### Scenario: No keep or dupe tokens in output
- **WHEN** any duplicate group is printed
- **THEN** stdout contains no line beginning with `  keep` or `  dupe`

### Progress Output

- Before scanning begins, the tool MUST emit to `stderr` the number of size groups
  with two or more candidates that will be examined.

### Summary Output

After all groups have been processed, the tool SHALL emit a summary line to `stderr` of the form:

```
<N> duplicate group(s) found, <X.X> MB of duplicated content
```

where `<N>` is the total number of content-duplicate groups printed, and `<X.X>` is the total duplicated space in mebibytes (1 MiB = 1,048,576 bytes), formatted to one decimal place. Duplicated space is calculated as `(AliasGroup count − 1) × file size` summed across all groups.

#### Scenario: Two groups of duplicates found
- **WHEN** two content-duplicate groups are found, each 2 MiB with two members
- **THEN** stderr ends with `2 duplicate group(s) found, 2.0 MB of duplicated content`

#### Scenario: No duplicates found
- **WHEN** no content-duplicate groups are found
- **THEN** stderr ends with `0 duplicate group(s) found, 0.0 MB of duplicated content`
