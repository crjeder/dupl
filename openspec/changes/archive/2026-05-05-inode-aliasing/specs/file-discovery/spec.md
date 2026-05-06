## MODIFIED Requirements

### Requirement: Size Grouping
Included files SHALL be grouped by their exact byte size. The unit of grouping is an `AliasGroup` (one logical file, which may have multiple path aliases). Any size group containing fewer than two `AliasGroup` values SHALL be discarded; it cannot contain duplicates. Only size groups with two or more `AliasGroup` values SHALL be returned to the caller.

#### Scenario: Two independent files with the same size
- **WHEN** two files with different inodes share the same byte size
- **THEN** they are placed in the same size group as separate `AliasGroup` values and forwarded to duplicate detection

#### Scenario: Two hard-linked paths with the same size
- **WHEN** two paths share the same inode (and therefore the same size)
- **THEN** they form one `AliasGroup`; the size group contains one entry and is discarded as a singleton

#### Scenario: Three paths — two aliased, one independent — sharing a size
- **WHEN** paths A and B share inode 42, and path C has inode 99, all with the same byte size
- **THEN** the size group contains two `AliasGroup` values (one with A+B, one with C) and is forwarded to duplicate detection
