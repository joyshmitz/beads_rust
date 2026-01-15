# Existing Beads Structure and Architecture

> Comprehensive analysis of the Go beads codebase for porting to Rust.

---

## Table of Contents

1. [Project Overview](#1-project-overview)
2. [Directory Structure](#2-directory-structure)
3. [Data Types and Models](#3-data-types-and-models)
4. [SQLite Storage Layer](#4-sqlite-storage-layer)
5. [CLI Commands](#5-cli-commands)
6. [JSONL Import/Export System](#6-jsonl-importexport-system)
7. [Dependency Graph and Blocking Logic](#7-dependency-graph-and-blocking-logic)
8. [Configuration System](#8-configuration-system)
9. [Key Architectural Patterns](#9-key-architectural-patterns)
10. [Porting Considerations](#10-porting-considerations)

---

## 1. Project Overview

**Location:** `./legacy_beads/` (gitignored reference copy)

**Statistics:**
- ~267,622 lines of Go code
- ~100 files in SQLite storage alone
- 40 database migrations
- 15+ CLI commands with extensive flag sets

**Core Architecture:**
- SQLite + JSONL hybrid storage
- Optional daemon mode with RPC (not porting initially)
- Content-addressable issues with hash-based IDs
- Git-integrated synchronization

---

## 2. Directory Structure

```
legacy_beads/
├── beads.go                    # Package root, version info
├── cmd/
│   └── bd/                     # CLI entry point (~100 files)
│       ├── create.go           # Issue creation
│       ├── update.go           # Issue updates
│       ├── close.go            # Issue closing
│       ├── list.go             # Issue listing
│       ├── show.go             # Issue details
│       ├── ready.go            # Ready work queries
│       ├── dep.go              # Dependency management
│       ├── sync.go             # Git synchronization
│       ├── daemon*.go          # Daemon mode (SKIP)
│       └── ...
├── internal/
│   ├── types/                  # Core data types
│   │   ├── types.go            # Issue, Dependency, etc. (42KB)
│   │   ├── id_generator.go     # Hash-based ID generation
│   │   └── lock.go             # Lock types
│   ├── storage/
│   │   ├── storage.go          # Storage interface (10KB)
│   │   ├── sqlite/             # SQLite implementation (PORT THIS)
│   │   │   ├── store.go        # Main storage struct
│   │   │   ├── schema.go       # Database schema
│   │   │   ├── queries.go      # SQL queries
│   │   │   ├── migrations/     # 40 migrations
│   │   │   └── ...
│   │   ├── dolt/               # Dolt backend (SKIP)
│   │   ├── memory/             # In-memory backend
│   │   └── factory/            # Backend factory
│   ├── export/                 # JSONL export logic
│   ├── autoimport/             # Auto-import from JSONL
│   ├── importer/               # Import logic
│   ├── compact/                # JSONL compaction
│   ├── configfile/             # Configuration handling
│   ├── validation/             # Input validation
│   ├── hooks/                  # Hook system
│   ├── git/                    # Git integration
│   ├── rpc/                    # RPC daemon (SKIP initially)
│   ├── linear/                 # Linear.app integration (SKIP)
│   └── ui/                     # Terminal UI helpers
└── docs/                       # Documentation
```

---

## 3. Data Types and Models

### 3.1 Issue Struct (Primary Entity)

The `Issue` struct has ~80 fields organized into logical groups:

```go
type Issue struct {
    // === Core Identification ===
    ID          string `json:"id"`           // Hash-based ID (e.g., "bd-abc123")
    ContentHash string `json:"-"`            // SHA256, NOT exported to JSONL

    // === Content Fields ===
    Title              string `json:"title"`                         // Required, max 500 chars
    Description        string `json:"description,omitempty"`
    Design             string `json:"design,omitempty"`
    AcceptanceCriteria string `json:"acceptance_criteria,omitempty"`
    Notes              string `json:"notes,omitempty"`

    // === Status & Workflow ===
    Status    Status    `json:"status,omitempty"`      // open, in_progress, blocked, closed, etc.
    Priority  int       `json:"priority"`              // 0-4 (P0-P4), NO omitempty (0 is valid)
    IssueType IssueType `json:"issue_type,omitempty"`  // task, bug, feature, epic, etc.

    // === Assignment ===
    Assignee         string `json:"assignee,omitempty"`
    Owner            string `json:"owner,omitempty"`           // Git author email for attribution
    EstimatedMinutes *int   `json:"estimated_minutes,omitempty"`

    // === Timestamps ===
    CreatedAt       time.Time  `json:"created_at"`
    CreatedBy       string     `json:"created_by,omitempty"`
    UpdatedAt       time.Time  `json:"updated_at"`
    ClosedAt        *time.Time `json:"closed_at,omitempty"`
    CloseReason     string     `json:"close_reason,omitempty"`
    ClosedBySession string     `json:"closed_by_session,omitempty"`  // Claude Code session

    // === Time-Based Scheduling ===
    DueAt      *time.Time `json:"due_at,omitempty"`       // When issue should complete
    DeferUntil *time.Time `json:"defer_until,omitempty"`  // Hide from bd ready until

    // === External Integration ===
    ExternalRef  *string `json:"external_ref,omitempty"`   // e.g., "gh-9", "jira-ABC"
    SourceSystem string  `json:"source_system,omitempty"`  // Federation source

    // === Compaction Metadata ===
    CompactionLevel   int        `json:"compaction_level,omitempty"`
    CompactedAt       *time.Time `json:"compacted_at,omitempty"`
    CompactedAtCommit *string    `json:"compacted_at_commit,omitempty"`
    OriginalSize      int        `json:"original_size,omitempty"`

    // === Internal Routing (NOT exported) ===
    SourceRepo     string `json:"-"`  // Which repo owns this issue
    IDPrefix       string `json:"-"`  // Override prefix for ID generation
    PrefixOverride string `json:"-"`  // Replace config prefix entirely

    // === Relational Data ===
    Labels       []string      `json:"labels,omitempty"`
    Dependencies []*Dependency `json:"dependencies,omitempty"`
    Comments     []*Comment    `json:"comments,omitempty"`

    // === Soft-Delete (Tombstone) ===
    DeletedAt    *time.Time `json:"deleted_at,omitempty"`
    DeletedBy    string     `json:"deleted_by,omitempty"`
    DeleteReason string     `json:"delete_reason,omitempty"`
    OriginalType string     `json:"original_type,omitempty"`

    // === Messaging/Ephemeral ===
    Sender    string `json:"sender,omitempty"`
    Ephemeral bool   `json:"ephemeral,omitempty"`  // If true, not exported to JSONL

    // === Context Markers ===
    Pinned     bool `json:"pinned,omitempty"`      // Persistent context
    IsTemplate bool `json:"is_template,omitempty"` // Read-only template

    // === Agent Identity Fields ===
    HookBead     string     `json:"hook_bead,omitempty"`     // Current work on hook
    RoleBead     string     `json:"role_bead,omitempty"`     // Role definition
    AgentState   AgentState `json:"agent_state,omitempty"`   // idle|running|stuck|stopped
    LastActivity *time.Time `json:"last_activity,omitempty"`
    RoleType     string     `json:"role_type,omitempty"`     // polecat|crew|witness|etc.
    Rig          string     `json:"rig,omitempty"`           // Multi-repo workspace

    // === Molecule/Work Type ===
    MolType  MolType  `json:"mol_type,omitempty"`   // swarm|patrol|work
    WorkType WorkType `json:"work_type,omitempty"`  // mutex|open_competition

    // === Gate Fields (Async Coordination) ===
    AwaitType string        `json:"await_type,omitempty"`  // gh:run, timer, human, mail
    AwaitID   string        `json:"await_id,omitempty"`
    Timeout   time.Duration `json:"timeout,omitempty"`
    Waiters   []string      `json:"waiters,omitempty"`     // Mail addresses to notify
    Holder    string        `json:"holder,omitempty"`      // For slots

    // === HOP Fields (Entity Tracking) ===
    Creator      *EntityRef   `json:"creator,omitempty"`
    Validations  []Validation `json:"validations,omitempty"`
    QualityScore *float32     `json:"quality_score,omitempty"`  // 0.0-1.0
    Crystallizes bool         `json:"crystallizes,omitempty"`   // Code vs ops

    // === Event Fields ===
    EventKind string `json:"event_kind,omitempty"`
    Actor     string `json:"actor,omitempty"`
    Target    string `json:"target,omitempty"`
    Payload   string `json:"payload,omitempty"`

    // === Bonding (Compound Molecules) ===
    BondedFrom []BondRef `json:"bonded_from,omitempty"`
}
```

### 3.2 Status Enum

```go
const (
    StatusOpen       Status = "open"
    StatusInProgress Status = "in_progress"
    StatusBlocked    Status = "blocked"
    StatusDeferred   Status = "deferred"
    StatusClosed     Status = "closed"
    StatusTombstone  Status = "tombstone"  // Soft-deleted
    StatusPinned     Status = "pinned"     // Persistent context
    StatusHooked     Status = "hooked"     // Attached to agent's hook
)
```

### 3.3 IssueType Enum

```go
const (
    TypeBug          IssueType = "bug"
    TypeFeature      IssueType = "feature"
    TypeTask         IssueType = "task"
    TypeEpic         IssueType = "epic"
    TypeChore        IssueType = "chore"
    TypeMessage      IssueType = "message"       // Ephemeral inter-worker
    TypeMergeRequest IssueType = "merge-request"
    TypeMolecule     IssueType = "molecule"      // Template for hierarchies
    TypeGate         IssueType = "gate"          // Async coordination
    TypeAgent        IssueType = "agent"         // Agent identity
    TypeRole         IssueType = "role"          // Agent role definition
    TypeRig          IssueType = "rig"           // Multi-repo workspace
    TypeConvoy       IssueType = "convoy"        // Cross-project tracking
    TypeEvent        IssueType = "event"         // Operational state change
    TypeSlot         IssueType = "slot"          // Exclusive access
)
```

### 3.4 Dependency Struct

```go
type Dependency struct {
    IssueID     string         `json:"issue_id"`
    DependsOnID string         `json:"depends_on_id"`
    Type        DependencyType `json:"type"`
    CreatedAt   time.Time      `json:"created_at"`
    CreatedBy   string         `json:"created_by,omitempty"`
    Metadata    string         `json:"metadata,omitempty"`   // Type-specific JSON
    ThreadID    string         `json:"thread_id,omitempty"`  // Conversation threading
}
```

### 3.5 DependencyType Enum

```go
const (
    // Workflow types (affect ready work calculation)
    DepBlocks            DependencyType = "blocks"
    DepParentChild       DependencyType = "parent-child"
    DepConditionalBlocks DependencyType = "conditional-blocks"
    DepWaitsFor          DependencyType = "waits-for"

    // Association types
    DepRelated        DependencyType = "related"
    DepDiscoveredFrom DependencyType = "discovered-from"

    // Graph link types
    DepRepliesTo  DependencyType = "replies-to"
    DepRelatesTo  DependencyType = "relates-to"
    DepDuplicates DependencyType = "duplicates"
    DepSupersedes DependencyType = "supersedes"

    // Entity types (HOP)
    DepAuthoredBy  DependencyType = "authored-by"
    DepAssignedTo  DependencyType = "assigned-to"
    DepApprovedBy  DependencyType = "approved-by"
    DepAttests     DependencyType = "attests"

    // Cross-project
    DepTracks DependencyType = "tracks"

    // Reference types
    DepUntil     DependencyType = "until"
    DepCausedBy  DependencyType = "caused-by"
    DepValidates DependencyType = "validates"

    // Delegation
    DepDelegatedFrom DependencyType = "delegated-from"
)

// AffectsReadyWork returns true for types that block ready work
func (t DependencyType) AffectsReadyWork() bool {
    return t == DepBlocks || t == DepParentChild ||
           t == DepConditionalBlocks || t == DepWaitsFor
}
```

### 3.6 Other Key Types

```go
// Comment represents a discussion entry
type Comment struct {
    ID        int64     `json:"id"`
    IssueID   string    `json:"issue_id"`
    Author    string    `json:"author"`
    Text      string    `json:"text"`
    CreatedAt time.Time `json:"created_at"`
}

// Event represents an audit trail entry
type Event struct {
    ID        int64     `json:"id"`
    IssueID   string    `json:"issue_id"`
    EventType EventType `json:"event_type"`
    Actor     string    `json:"actor"`
    OldValue  *string   `json:"old_value,omitempty"`
    NewValue  *string   `json:"new_value,omitempty"`
    Comment   *string   `json:"comment,omitempty"`
    CreatedAt time.Time `json:"created_at"`
}

// EventType constants
const (
    EventCreated           EventType = "created"
    EventUpdated           EventType = "updated"
    EventStatusChanged     EventType = "status_changed"
    EventCommented         EventType = "commented"
    EventClosed            EventType = "closed"
    EventReopened          EventType = "reopened"
    EventDependencyAdded   EventType = "dependency_added"
    EventDependencyRemoved EventType = "dependency_removed"
    EventLabelAdded        EventType = "label_added"
    EventLabelRemoved      EventType = "label_removed"
    EventCompacted         EventType = "compacted"
)

// Statistics for project overview
type Statistics struct {
    TotalIssues             int     `json:"total_issues"`
    OpenIssues              int     `json:"open_issues"`
    InProgressIssues        int     `json:"in_progress_issues"`
    ClosedIssues            int     `json:"closed_issues"`
    BlockedIssues           int     `json:"blocked_issues"`
    DeferredIssues          int     `json:"deferred_issues"`
    ReadyIssues             int     `json:"ready_issues"`
    TombstoneIssues         int     `json:"tombstone_issues"`
    PinnedIssues            int     `json:"pinned_issues"`
    EpicsEligibleForClosure int     `json:"epics_eligible_for_closure"`
    AverageLeadTime         float64 `json:"average_lead_time_hours"`
}

// IssueFilter for search queries (26+ filter options)
type IssueFilter struct {
    Status, Priority, IssueType        string
    Assignee, Label, Query             string
    CreatedAfter, CreatedBefore        *time.Time
    UpdatedAfter, UpdatedBefore        *time.Time
    ClosedAfter, ClosedBefore          *time.Time
    HasDescription, HasNotes           *bool
    IncludeTombstones, IncludeEphemeral bool
    ParentID, MolType                  string
    ExcludeStatuses, ExcludeTypes      []string
    Overdue, DeferredOnly, PinnedOnly  bool
    Limit                              int
    // ... more fields
}
```

### 3.7 Content Hashing

The `ComputeContentHash()` method creates a deterministic SHA256 hash for deduplication:

**Included fields** (in order):
1. Title, Description, Design, AcceptanceCriteria, Notes
2. Status, Priority, IssueType
3. Assignee, Owner, CreatedBy
4. ExternalRef, SourceSystem
5. Pinned, IsTemplate flags
6. BondedFrom entries
7. Creator EntityRef
8. Validations with scores
9. QualityScore, Crystallizes
10. Gate fields (AwaitType, AwaitID, Timeout, Waiters)
11. Holder, HookBead, RoleBead, AgentState, RoleType, Rig
12. MolType, WorkType
13. Event fields

**NOT included:** ID, timestamps, compaction metadata (these change without content change)

---

## 4. SQLite Storage Layer

### 4.1 Database Schema

#### Issues Table (Core)

```sql
CREATE TABLE issues (
    id TEXT PRIMARY KEY,
    content_hash TEXT,
    title TEXT NOT NULL CHECK(length(title) <= 500),
    description TEXT NOT NULL DEFAULT '',
    design TEXT NOT NULL DEFAULT '',
    acceptance_criteria TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'open',
    priority INTEGER NOT NULL DEFAULT 2 CHECK(priority >= 0 AND priority <= 4),
    issue_type TEXT NOT NULL DEFAULT 'task',
    assignee TEXT,
    owner TEXT DEFAULT '',
    estimated_minutes INTEGER,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by TEXT DEFAULT '',
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    closed_at DATETIME,
    close_reason TEXT DEFAULT '',
    closed_by_session TEXT DEFAULT '',
    external_ref TEXT,
    due_at DATETIME,
    defer_until DATETIME,
    -- Compaction
    compaction_level INTEGER DEFAULT 0,
    compacted_at DATETIME,
    compacted_at_commit TEXT,
    original_size INTEGER,
    -- Tombstone
    deleted_at DATETIME,
    deleted_by TEXT DEFAULT '',
    delete_reason TEXT DEFAULT '',
    original_type TEXT DEFAULT '',
    -- Messaging
    sender TEXT DEFAULT '',
    ephemeral INTEGER DEFAULT 0,
    -- Context
    pinned INTEGER DEFAULT 0,
    is_template INTEGER DEFAULT 0,
    -- Agent fields
    hook_bead TEXT DEFAULT '',
    role_bead TEXT DEFAULT '',
    agent_state TEXT DEFAULT '',
    last_activity DATETIME,
    role_type TEXT DEFAULT '',
    rig TEXT DEFAULT '',
    -- Molecule
    mol_type TEXT DEFAULT '',
    work_type TEXT DEFAULT 'mutex',
    -- HOP
    crystallizes INTEGER DEFAULT 0,
    quality_score REAL,
    -- Event
    event_kind TEXT DEFAULT '',
    actor TEXT DEFAULT '',
    target TEXT DEFAULT '',
    payload TEXT DEFAULT '',
    source_system TEXT DEFAULT '',

    -- Constraint: closed_at invariant
    CHECK (
        (status = 'closed' AND closed_at IS NOT NULL) OR
        (status = 'tombstone') OR
        (status NOT IN ('closed', 'tombstone') AND closed_at IS NULL)
    )
);

-- Key indexes
CREATE INDEX idx_issues_status ON issues(status);
CREATE INDEX idx_issues_priority ON issues(priority);
CREATE INDEX idx_issues_assignee ON issues(assignee);
CREATE INDEX idx_issues_created_at ON issues(created_at);
CREATE INDEX idx_issues_external_ref ON issues(external_ref);
CREATE INDEX idx_issues_content_hash ON issues(content_hash);
```

#### Dependencies Table

```sql
CREATE TABLE dependencies (
    issue_id TEXT NOT NULL,
    depends_on_id TEXT NOT NULL,
    type TEXT NOT NULL DEFAULT 'blocks',
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',
    thread_id TEXT DEFAULT '',

    PRIMARY KEY (issue_id, depends_on_id),
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
    -- Note: depends_on_id FK removed in migration 025 to allow external refs
);

CREATE INDEX idx_dependencies_issue ON dependencies(issue_id);
CREATE INDEX idx_dependencies_depends_on ON dependencies(depends_on_id);
CREATE INDEX idx_dependencies_depends_on_type ON dependencies(depends_on_id, type);
CREATE INDEX idx_dependencies_thread ON dependencies(thread_id) WHERE thread_id != '';
```

#### Labels Table

```sql
CREATE TABLE labels (
    issue_id TEXT NOT NULL,
    label TEXT NOT NULL,
    PRIMARY KEY (issue_id, label),
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

CREATE INDEX idx_labels_label ON labels(label);
```

#### Comments Table

```sql
CREATE TABLE comments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    issue_id TEXT NOT NULL,
    author TEXT NOT NULL,
    text TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

CREATE INDEX idx_comments_issue ON comments(issue_id);
```

#### Events Table (Audit Trail)

```sql
CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    issue_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    actor TEXT NOT NULL,
    old_value TEXT,
    new_value TEXT,
    comment TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

CREATE INDEX idx_events_issue ON events(issue_id);
CREATE INDEX idx_events_created_at ON events(created_at);
```

#### Config Table

```sql
CREATE TABLE config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

#### Metadata Table

```sql
CREATE TABLE metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

#### Dirty Issues Table (Export Tracking)

```sql
CREATE TABLE dirty_issues (
    issue_id TEXT PRIMARY KEY,
    marked_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);

CREATE INDEX idx_dirty_issues_marked_at ON dirty_issues(marked_at);
```

#### Export Hashes Table (Deduplication)

```sql
CREATE TABLE export_hashes (
    issue_id TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    exported_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE
);
```

#### Child Counters Table (Hierarchical IDs)

```sql
CREATE TABLE child_counters (
    parent_id TEXT PRIMARY KEY,
    last_child INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (parent_id) REFERENCES issues(id) ON DELETE CASCADE
);
```

### 4.2 SQLite Pragmas and Configuration

```sql
-- Connection-level pragmas
PRAGMA foreign_keys = ON;        -- Enforce referential integrity
PRAGMA busy_timeout = 30000;     -- 30s timeout for locked database
PRAGMA journal_mode = WAL;       -- Write-Ahead Logging for concurrency

-- Exception: Use DELETE mode for:
-- - WSL2 with Windows filesystem (/mnt/c/, etc.)
-- - In-memory databases (:memory:)
```

**Connection Pool Settings:**
- File-based: `MaxOpenConns = NumCPU() + 1`, `MaxIdleConns = 2`
- In-memory: `MaxOpenConns = 1`, `MaxIdleConns = 1`

### 4.3 Transaction Handling

```go
// IMMEDIATE mode for write operations (acquire lock early)
func beginImmediateWithRetry(ctx, conn, retries, baseDelay) error {
    for attempt := 0; attempt < retries; attempt++ {
        if err := conn.ExecContext(ctx, "BEGIN IMMEDIATE"); err == nil {
            return nil
        }
        delay := baseDelay * (1 << attempt)  // Exponential backoff
        time.Sleep(delay)
    }
    return error
}
```

### 4.4 Migration System

40 migrations in order, each idempotent:

1. `dirty_issues_table` - Export tracking
2. `external_ref_column` - External references
3. `composite_indexes` - Performance indexes
4. `closed_at_constraint` - Status invariant
5. `compaction_columns` - AI summarization
6. `snapshots_table` - Compaction snapshots
7. `compaction_config` - Config values
8. `compacted_at_commit_column` - Git commit tracking
9. `export_hashes_table` - Dedup tracking
10. `content_hash_column` - Content hashing
11. `external_ref_unique` - Unique constraint
12. `source_repo_column` - Multi-repo support
13. `repo_mtimes_table` - Multi-repo optimization
14. `child_counters_table` - Hierarchical IDs
15. `blocked_issues_cache` - Performance cache
16. `orphan_detection` - Orphan handling
17. `close_reason_column` - Close reasons
18. `tombstone_columns` - Soft delete
19. `messaging_fields` - Inter-agent messaging
20. `edge_consolidation` - Dependency metadata
21. `migrate_edge_fields` - Edge field migration
22. `drop_edge_columns` - Remove old columns
23. `pinned_column` - Context markers
24. `is_template_column` - Templates
25. `remove_depends_on_fk` - External refs support
26. `additional_indexes` - More indexes
27. `gate_columns` - Async coordination
28. `tombstone_closed_at` - Fix tombstone handling
29. `created_by_column` - Creator tracking
30. `agent_fields` - Agent identity
31. `mol_type_column` - Molecule types
32. `hooked_status_migration` - Hooked status
33. `event_fields` - Event tracking
34. `closed_by_session_column` - Session tracking
35. `due_defer_columns` - Time scheduling
36. `owner_column` - Owner field
37. `crystallizes_column` - HOP field
38. `work_type_column` - Assignment model
39. `source_system_column` - Federation
40. `quality_score_column` - Quality tracking

---

## 5. CLI Commands

### 5.1 Command Overview

| Command | Purpose | Key Flags |
|---------|---------|-----------|
| `create` | Create issue | `--type`, `--priority`, `--parent`, `--deps`, `--labels` |
| `update` | Update issue | `--status`, `--priority`, `--assignee`, `--add-label` |
| `close` | Close issue | `--reason`, `--force`, `--suggest-next` |
| `list` | List issues | `--status`, `--priority`, `--label`, `--pretty` |
| `show` | Show details | `--short`, `--thread`, `--refs` |
| `ready` | Ready work | `--limit`, `--assignee`, `--sort`, `--mol` |
| `dep` | Dependencies | `add`, `remove`, `list`, `tree`, `cycles` |
| `sync` | Git sync | `--flush-only`, `--import-only`, `--dry-run` |
| `config` | Configuration | `get`, `set`, `list` |
| `init` | Initialize | Creates `.beads/` directory |
| `stats` | Statistics | Show project stats |
| `blocked` | Blocked issues | Show blocked issues |
| `count` | Count issues | Count by filter |
| `delete` | Delete issue | `--force`, `--hard` |
| `compact` | Compact JSONL | AI summarization |

### 5.2 Global Flags

```
--db              Database path (auto-discover .beads/*.db)
--actor           Actor name for audit trail
--json            Output in JSON format
--no-daemon       Force direct storage mode
--no-auto-flush   Skip automatic JSONL sync
--no-auto-import  Skip auto JSONL import
--verbose, -v     Verbose debug output
--quiet, -q       Suppress non-essential output
--lock-timeout    SQLite busy timeout (default: 30s)
```

### 5.3 Create Command

```bash
bd create "Issue title" [flags]

Flags:
  --type, -t          Issue type (task|bug|feature|epic|chore|...)
  --priority, -p      Priority P0-P4 (default: P2)
  --description, -d   Description text
  --design            Design specification
  --acceptance        Acceptance criteria
  --notes             Additional notes
  --assignee          Assign to person
  --labels, -l        Comma-separated labels
  --parent            Parent issue ID
  --deps              Dependencies (type:id format)
  --estimate, -e      Time estimate in minutes
  --due               Due date/time
  --defer             Defer until date
  --ephemeral         Not exported to JSONL
  --dry-run           Preview without creating
  --silent            Output only issue ID
```

### 5.4 Ready Command

```bash
bd ready [flags]

Flags:
  --limit            Max results
  --assignee         Filter by assignee
  --unassigned       Show unassigned only
  --sort             Sort policy (hybrid|priority|oldest)
  --label, -l        Filter by labels (AND)
  --label-any        Filter by labels (OR)
  --type             Filter by issue type
  --priority         Filter by priority
  --pretty           Pretty tree format
  --include-deferred Include deferred issues
  --mol              Filter to specific molecule
```

### 5.5 Dep Command

```bash
bd dep add <issue> <depends-on> [--type blocks|parent-child|related|...]
bd dep remove <issue> <depends-on>
bd dep list <issue> [--direction down|up]
bd dep tree <issue> [--max-depth N] [--format mermaid]
bd dep cycles
```

### 5.6 Sync Command

```bash
bd sync [flags]

Flags:
  --flush-only     Just export to JSONL
  --import-only    Just import from JSONL
  --dry-run        Preview changes
  --no-pull        Export-only (skip git pull)
  --no-push        Skip git push
  --squash         Export but skip git
  --status         Show sync status
  --message        Custom commit message
```

### 5.7 Output Formatting

**Text Mode (default):**
- Status icons: `○` (open), `◐` (in_progress), `●` (blocked), `✓` (closed), `❄` (deferred)
- Priority colors: P0 (red), P1 (orange), P2 (yellow), P3 (blue), P4 (gray)
- Tree rendering with Unicode box-drawing: `├──`, `└──`, `│`

**JSON Mode (`--json`):**
- Raw JSON matching Go struct definitions
- Suitable for piping to `jq` or other tools

---

## 6. JSONL Import/Export System

### 6.1 File Format

**Location:** `.beads/issues.jsonl`

One issue per line, complete JSON serialization:

```json
{"id":"bd-abc123","title":"Fix bug","status":"open","priority":1,...}
{"id":"bd-def456","title":"Add feature","status":"closed","priority":2,...}
```

### 6.2 Export Flow

```
Database (SQLite)
    ↓
Get all issues (including tombstones)
    ↓
Populate dependencies, labels, comments
    ↓
Compute content hashes
    ↓
Write to temp file (atomic)
    ↓
Rename to issues.jsonl
    ↓
Update export hashes
    ↓
Clear dirty flags
```

### 6.3 Import Flow

```
JSONL file
    ↓
Staleness check (mtime + content hash)
    ↓
Merge conflict detection (git markers)
    ↓
Parse JSONL (stream, 2MB buffer)
    ↓
Normalize (canonicalize refs, compute hashes)
    ↓
Collision detection & remapping
    ↓
Upsert issues, dependencies, labels, comments
    ↓
Checkpoint WAL
    ↓
Update import metadata
```

### 6.4 Dirty Tracking

Issues are marked dirty on:
- Create, Update, Close
- Dependency add/remove
- Label add/remove
- Comment addition

**dirty_issues table** tracks which issues need export.

### 6.5 Content Hash Tracking

- `export_hashes` table: Last exported hash per issue
- `jsonl_content_hash` metadata: Hash of entire JSONL file
- Enables incremental export and external change detection

### 6.6 3-Way Merge (Sync)

```
Base: .beads/sync_base.jsonl (last successful sync)
Local: Current database state
Remote: JSONL after git pull

Merge strategy: Last-Write-Wins (LWW) - later timestamp wins
```

---

## 7. Dependency Graph and Blocking Logic

### 7.1 Blocking Calculation

**Issue is blocked if:**
- Has `blocks` dependency on open issue
- Parent has `parent-child` dependency on open issue
- Has `conditional-blocks` on issue that hasn't failed
- Has `waits-for` with pending children

```sql
-- Ready issues view (simplified)
SELECT i.*
FROM issues i
WHERE i.status = 'open'
  AND NOT EXISTS (
    SELECT 1 FROM dependencies d
    JOIN issues blocker ON d.depends_on_id = blocker.id
    WHERE d.issue_id = i.id
      AND d.type IN ('blocks', 'parent-child')
      AND blocker.status IN ('open', 'in_progress', 'blocked', 'deferred')
  );
```

### 7.2 Cycle Detection

Uses recursive CTE with depth limit:

```sql
WITH RECURSIVE paths AS (
    SELECT issue_id, depends_on_id, 1 as depth
    FROM dependencies
    WHERE issue_id = ?

    UNION ALL

    SELECT d.issue_id, d.depends_on_id, p.depth + 1
    FROM dependencies d
    JOIN paths p ON d.issue_id = p.depends_on_id
    WHERE p.depth < 100
)
SELECT EXISTS(SELECT 1 FROM paths WHERE depends_on_id = ?);
```

### 7.3 Dependency Tree

```
bd-root
├── bd-child1 [P0] (open)
│   └── bd-grandchild [P1] (blocked)
└── bd-child2 [P2] (closed)
```

### 7.4 Parent-Child Semantics

- Child `depends_on` parent (child is blocked until parent closes)
- Alternatively, child has `parent-child` dependency pointing to parent
- Dotted ID format: `bd-abc.1`, `bd-abc.1.2`

---

## 8. Configuration System

### 8.1 Config Keys

| Key | Default | Purpose |
|-----|---------|---------|
| `issue_prefix` | `"bd"` | Issue ID prefix |
| `status.custom` | `""` | Comma-separated custom statuses |
| `types.custom` | `""` | Comma-separated custom types |
| `import.orphan_handling` | `"allow"` | `allow`, `skip`, `strict`, `resurrect` |
| `compaction_enabled` | `"false"` | Enable AI compaction |
| `compact_tier1_days` | `30` | Days before tier-1 compaction |
| `compact_model` | `"claude-3-5-haiku"` | AI model for compaction |

### 8.2 Metadata Keys

| Key | Purpose |
|-----|---------|
| `jsonl_content_hash` | SHA256 of JSONL file |
| `last_import_time` | RFC3339Nano timestamp |
| `jsonl_file_hash` | Previous file hash |

---

## 9. Key Architectural Patterns

### 9.1 Dual-Mode Architecture

**Daemon Mode (RPC):**
- Client sends Request → Daemon → Response
- Shared database connection
- Auto-import handling
- Used by default when daemon is running

**Direct Mode (SQLite):**
- Bypasses daemon, opens SQLite directly
- Used with `--no-daemon` flag
- Better for read-only operations

### 9.2 Last-Touched Issue

Commands without args default to last touched issue:
- Tracked via `SetLastTouchedID()`
- Commands that touch: create, update, close, show

### 9.3 Partial ID Resolution

Users can provide short IDs:
- `bd-abc` matches `bd-abc123`
- First exact match, then prefix matching

### 9.4 Cross-Rig Routing

Multi-repo support:
- `--rig` / `--prefix` flags route to different `.beads/` directories
- `routes.jsonl` for discovery
- Prefix inheritance for new repos

### 9.5 Atomic File Operations

All JSONL writes:
1. Write to temp file
2. Close file
3. Atomic rename to final path
4. Set permissions (0600)

---

## 10. Porting Considerations

### 10.1 Rust Type Mapping

| Go | Rust |
|----|------|
| `string` | `String` |
| `*string` | `Option<String>` |
| `int` | `i32` |
| `*int` | `Option<i32>` |
| `float32` | `f32` |
| `*float32` | `Option<f32>` |
| `bool` | `bool` |
| `time.Time` | `DateTime<Utc>` (chrono) |
| `*time.Time` | `Option<DateTime<Utc>>` |
| `time.Duration` | `Duration` (std::time) |
| `[]string` | `Vec<String>` |
| `map[string]interface{}` | `HashMap<String, Value>` |

### 10.2 Key Crates

| Purpose | Crate |
|---------|-------|
| CLI | `clap` with derive |
| SQLite | `rusqlite` (bundled) |
| JSON | `serde` + `serde_json` |
| Time | `chrono` |
| Hashing | `sha2` |
| Parallel | `rayon` |
| Logging | `tracing` |
| Errors | `anyhow` + `thiserror` |

### 10.3 Schema Compatibility

Database schema must match Go beads for potential cross-tool usage:
- Same table names and columns
- Same data types (SQLite is loosely typed)
- Same constraints and indexes

### 10.4 Output Compatibility

JSON output must be identical:
- Same field names (use `#[serde(rename)]` if needed)
- Same serialization order
- Same handling of optional fields

### 10.5 What NOT to Port

- **Dolt backend** (`internal/storage/dolt/`)
- **RPC daemon** (`cmd/bd/daemon*.go`) - initially
- **Linear integration** (`internal/linear/`)
- **Claude plugin** (separate project)

### 10.6 Priority Order

1. Data types and models
2. SQLite storage (schema, queries, migrations)
3. Basic CLI (create, list, show, update, close, ready)
4. Dependency management (dep add/remove/list/tree)
5. JSONL export/import
6. Sync command
7. Advanced features (compaction, hooks, etc.)

---

## Appendix A: Important Invariants

### Closed-At Invariant

```
IF Status == Closed THEN ClosedAt MUST be set
IF Status != Closed AND Status != Tombstone THEN ClosedAt MUST NOT be set
```

### Tombstone Invariant

```
IF Status == Tombstone THEN DeletedAt MUST be set
IF Status != Tombstone THEN DeletedAt MUST NOT be set
```

### Priority Range

```
0 <= Priority <= 4
```

### Title Length

```
len(Title) <= 500
```

### Cycle Prevention

Blocking dependencies (`blocks`, `parent-child`, `conditional-blocks`, `waits-for`) cannot form cycles.

---

## Appendix B: Error Handling

### Sentinel Errors

```go
ErrNotFound    = errors.New("not found")
ErrInvalidID   = errors.New("invalid ID")
ErrConflict    = errors.New("conflict")
ErrCycle       = errors.New("dependency cycle detected")
```

### Error Policies

```go
PolicyStrict       // Fail-fast on any error
PolicyBestEffort   // Skip errors with warnings
PolicyPartial      // Retry, then skip with manifest
PolicyRequiredCore // Fail on core, skip enrichments
```

---

## Appendix C: SQLite Connection String

```
file:path/to/beads.db?
  _pragma=foreign_keys(ON)&
  _pragma=busy_timeout(30000)&
  _pragma=journal_mode(WAL)&
  _time_format=sqlite
```

---

*Document generated for beads_rust porting project. Last updated: 2026-01-15*
