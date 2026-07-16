const std = @import("std");
const builtin = @import("builtin");

const version = "0.4.0";

const OutputFormat = enum { json, table, plain };
const ArchiveFilter = enum { active, archived, all };
const IssueSeverity = enum { info, warning, err };

const Issue = struct {
    code: []const u8,
    severity: IssueSeverity,
    filename: []const u8,
    line: usize,
    field: []const u8,
    message: []const u8,
    repairable: bool,
};

const IdeaMeta = struct {
    schema: u16,
    has_schema: bool,
    has_id: bool,
    has_kind: bool,
    id: []const u8,
    filename: []const u8,
    project: []const u8,
    kind: []const u8,
    timestamp: i64,
    created_at_ns: i128,
    title: []const u8,
    tags: []const u8,
    priority: []const u8,
    archived_at: i64,
    resolution: []const u8,
    resolution_note: []const u8,
    body: []const u8,
    score: usize,

    fn lessThan(_: void, a: IdeaMeta, b: IdeaMeta) bool {
        if (a.timestamp != b.timestamp) return a.timestamp > b.timestamp;
        if (a.created_at_ns != b.created_at_ns) return a.created_at_ns > b.created_at_ns;
        return std.mem.order(u8, a.filename, b.filename) == .gt;
    }

    fn contextLessThan(_: void, a: IdeaMeta, b: IdeaMeta) bool {
        const a_rank = priority_rank(a.priority);
        const b_rank = priority_rank(b.priority);
        if (a_rank != b_rank) return a_rank > b_rank;
        return lessThan({}, a, b);
    }

    fn searchLessThan(_: void, a: IdeaMeta, b: IdeaMeta) bool {
        if (a.score != b.score) return a.score > b.score;
        return contextLessThan({}, a, b);
    }
};

fn priority_rank(priority: []const u8) u8 {
    if (std.mem.eql(u8, priority, "high")) return 3;
    if (std.mem.eql(u8, priority, "low")) return 1;
    return 2;
}

fn truncate_title(allocator: std.mem.Allocator, text: []const u8) ![]const u8 {
    const first_line_end = std.mem.indexOfAny(u8, text, "\r\n") orelse text.len;
    const line = std.mem.trim(u8, text[0..first_line_end], " \t");
    var limit = @min(line.len, 60);
    if (limit < line.len) {
        while (limit > 0 and (line[limit] & 0xC0) == 0x80) limit -= 1;
    }
    return allocator.dupe(u8, line[0..limit]);
}

fn get_default_title(allocator: std.mem.Allocator, text: []const u8) ![]const u8 {
    var lines = std.mem.splitScalar(u8, text, '\n');
    while (lines.next()) |raw_line| {
        const line = std.mem.trim(u8, raw_line, " \t\r");
        var hashes: usize = 0;
        while (hashes < line.len and hashes < 6 and line[hashes] == '#') hashes += 1;
        if (hashes > 0 and hashes < line.len and line[hashes] == ' ') {
            const heading = std.mem.trim(u8, line[hashes + 1 ..], " \t");
            if (heading.len > 0) return truncate_title(allocator, heading);
        }
    }
    return truncate_title(allocator, text);
}

fn derive_id(allocator: std.mem.Allocator, seed: []const u8) ![]const u8 {
    var hash: u64 = 14695981039346656037;
    for (seed) |byte| {
        hash ^= byte;
        hash *%= 1099511628211;
    }
    return std.fmt.allocPrint(allocator, "{x:0>12}", .{hash & 0xffffffffffff});
}

fn print_escaped_json(writer: *std.Io.Writer, s: []const u8) !void {
    for (s) |c| {
        switch (c) {
            '\\' => try writer.writeAll("\\\\"),
            '\"' => try writer.writeAll("\\\""),
            '\n' => try writer.writeAll("\\n"),
            '\r' => try writer.writeAll("\\r"),
            '\t' => try writer.writeAll("\\t"),
            else => {
                if (c < 0x20) {
                    try writer.print("\\u{x:0>4}", .{c});
                } else {
                    try writer.writeByte(c);
                }
            },
        }
    }
}

fn emit_tags_json(writer: *std.Io.Writer, tags: []const u8) !void {
    try writer.writeAll("[");
    var first = true;
    var pos: usize = 0;
    while (pos < tags.len) {
        const end = std.mem.indexOfScalarPos(u8, tags, pos, ',') orelse tags.len;
        const tag = std.mem.trim(u8, tags[pos..end], " ");
        if (tag.len > 0) {
            if (!first) try writer.writeAll(",");
            try writer.writeAll("\"");
            try print_escaped_json(writer, tag);
            try writer.writeAll("\"");
            first = false;
        }
        if (end >= tags.len) break;
        pos = end + 1;
    }
    try writer.writeAll("]");
}

fn emit_idea_json(writer: *std.Io.Writer, meta: IdeaMeta, first: bool) !void {
    if (!first) try writer.writeAll(",");
    try writer.writeAll("{\"id\":\"");
    try print_escaped_json(writer, meta.id);
    try writer.writeAll("\",\"filename\":\"");
    try print_escaped_json(writer, meta.filename);
    try writer.writeAll("\",\"project\":\"");
    try print_escaped_json(writer, meta.project);
    try writer.writeAll("\",\"kind\":\"");
    try print_escaped_json(writer, meta.kind);
    try writer.writeAll("\",\"title\":\"");
    try print_escaped_json(writer, meta.title);
    try writer.print("\",\"timestamp\":{d}", .{meta.timestamp});
    if (meta.created_at_ns > 0) try writer.print(",\"created_at_ns\":{d}", .{meta.created_at_ns});
    try writer.writeAll(",\"tags\":");
    try emit_tags_json(writer, meta.tags);
    if (meta.priority.len > 0) {
        try writer.writeAll(",\"priority\":\"");
        try print_escaped_json(writer, meta.priority);
        try writer.writeAll("\"");
    }
    if (meta.archived_at > 0) {
        try writer.print(",\"archived_at\":{d}", .{meta.archived_at});
        if (meta.resolution.len > 0) {
            try writer.writeAll(",\"resolution\":\"");
            try print_escaped_json(writer, meta.resolution);
            try writer.writeAll("\"");
        }
        if (meta.resolution_note.len > 0) {
            try writer.writeAll(",\"resolution_note\":\"");
            try print_escaped_json(writer, meta.resolution_note);
            try writer.writeAll("\"");
        }
    }
    if (meta.score > 0) try writer.print(",\"score\":{d}", .{meta.score});
    try writer.writeAll("}");
}

fn emit_idea_plain(writer: *std.Io.Writer, meta: IdeaMeta) !void {
    try writer.print("{s}  {s:<11}  {s}", .{ meta.id, meta.kind, meta.title });
    if (meta.tags.len > 0) try writer.print("  [{s}]", .{meta.tags});
    if (meta.priority.len > 0) try writer.print("  ({s})", .{meta.priority});
    try writer.writeAll("\n");
}

fn emit_idea_table(writer: *std.Io.Writer, meta: IdeaMeta) !void {
    // Truncate title to 40 chars for table display
    var title_limit = @min(meta.title.len, 40);
    if (title_limit < meta.title.len) {
        while (title_limit > 0 and (meta.title[title_limit] & 0xC0) == 0x80) {
            title_limit -= 1;
        }
    }
    const display_title = meta.title[0..title_limit];
    const ellipsis: []const u8 = if (title_limit < meta.title.len) "..." else "";

    // Format timestamp as YYYY-MM-DD
    if (meta.timestamp > 0) {
        const epoch_secs = std.time.epoch.EpochSeconds{ .secs = @intCast(meta.timestamp) };
        const epoch_day = epoch_secs.getEpochDay();
        const year_day = epoch_day.calculateYearDay();
        const month_day = year_day.calculateMonthDay();
        try writer.print("{d:0>4}-{d:0>2}-{d:0>2}", .{ year_day.year, @intFromEnum(month_day.month), month_day.day_index + 1 });
    } else {
        try writer.writeAll("          ");
    }

    try writer.print("  {s:<16}  {s:<11}  {s:<12}  {s}{s}", .{ meta.project, meta.kind, meta.id, display_title, ellipsis });
    if (meta.tags.len > 0) try writer.print("  [{s}]", .{meta.tags});
    if (meta.priority.len > 0) try writer.print("  ({s})", .{meta.priority});
    try writer.writeAll("\n");
}

fn escape_yaml_string(arena: std.mem.Allocator, value: []const u8) ![]const u8 {
    var result: std.ArrayList(u8) = .empty;
    for (value) |c| {
        switch (c) {
            '\\', '"' => {
                try result.append(arena, '\\');
                try result.append(arena, c);
            },
            '\n' => try result.appendSlice(arena, "\\n"),
            '\r' => try result.appendSlice(arena, "\\r"),
            '\t' => try result.appendSlice(arena, "\\t"),
            else => try result.append(arena, c),
        }
    }
    return result.items;
}

fn unescape_yaml_string(arena: std.mem.Allocator, value: []const u8) ![]const u8 {
    const stripped = strip_yaml_quotes(value);
    // If it doesn't contain any backslashes, we don't need to allocate/unescape
    if (std.mem.indexOfScalar(u8, stripped, '\\') == null) {
        return try arena.dupe(u8, stripped);
    }
    var result: std.ArrayList(u8) = .empty;
    var i: usize = 0;
    while (i < stripped.len) {
        if (stripped[i] == '\\' and i + 1 < stripped.len) {
            const next = stripped[i + 1];
            const decoded: ?u8 = switch (next) {
                '"', '\\' => next,
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                else => null,
            };
            if (decoded) |c| {
                try result.append(arena, c);
                i += 2;
                continue;
            }
        }
        try result.append(arena, stripped[i]);
        i += 1;
    }
    return result.items;
}

fn add_issue(
    arena: std.mem.Allocator,
    issues: ?*std.ArrayList(Issue),
    filename: []const u8,
    line: usize,
    code: []const u8,
    severity: IssueSeverity,
    field: []const u8,
    repairable: bool,
    comptime fmt: []const u8,
    args: anytype,
) !void {
    const list = issues orelse return;
    try list.append(arena, .{
        .code = code,
        .severity = severity,
        .filename = try arena.dupe(u8, filename),
        .line = line,
        .field = field,
        .message = try std.fmt.allocPrint(arena, fmt, args),
        .repairable = repairable,
    });
}

fn canonical_kind(value: []const u8) ?[]const u8 {
    inline for (.{ "technical", "product", "business", "project", "unspecified" }) |kind| {
        if (std.ascii.eqlIgnoreCase(value, kind)) return kind;
    }
    return null;
}

fn canonical_priority(value: []const u8) ?[]const u8 {
    inline for (.{ "low", "medium", "high" }) |priority| {
        if (std.ascii.eqlIgnoreCase(value, priority)) return priority;
    }
    return null;
}

fn valid_resolution(value: []const u8) bool {
    return std.mem.eql(u8, value, "implemented") or
        std.mem.eql(u8, value, "rejected") or
        std.mem.eql(u8, value, "superseded") or
        std.mem.eql(u8, value, "stale");
}

fn valid_id(value: []const u8) bool {
    if (value.len != 12) return false;
    for (value) |c| if (!std.ascii.isHex(c)) return false;
    return true;
}

fn note_field(
    arena: std.mem.Allocator,
    issues: ?*std.ArrayList(Issue),
    filename: []const u8,
    line: usize,
    key: []const u8,
    seen: *bool,
) !void {
    if (seen.*) try add_issue(arena, issues, filename, line, "duplicate_field", .err, key, false, "Duplicate front-matter field '{s}'", .{key});
    seen.* = true;
}

/// Parse the constrained YAML front matter used by pin and optionally collect diagnostics.
fn parse_front_matter_detailed(
    arena: std.mem.Allocator,
    content: []const u8,
    filename: []const u8,
    issues: ?*std.ArrayList(Issue),
) !?IdeaMeta {
    if (content.len < 4) {
        try add_issue(arena, issues, filename, 1, "missing_front_matter", .err, "", false, "Missing front matter", .{});
        return null;
    }

    const first_nl = std.mem.indexOfScalar(u8, content, '\n') orelse {
        try add_issue(arena, issues, filename, 1, "missing_front_matter", .err, "", false, "Missing front matter", .{});
        return null;
    };
    const first_line = std.mem.trim(u8, content[0..first_nl], " \r");
    if (!std.mem.eql(u8, first_line, "---")) {
        try add_issue(arena, issues, filename, 1, "missing_front_matter", .err, "", false, "Missing opening front-matter delimiter", .{});
        return null;
    }

    var schema: u16 = 0;
    var id: ?[]const u8 = null;
    var project: ?[]const u8 = null;
    var kind: ?[]const u8 = null;
    var timestamp: ?i64 = null;
    var created_at_ns: ?i128 = null;
    var title: ?[]const u8 = null;
    var tags: ?[]const u8 = null;
    var priority: ?[]const u8 = null;
    var archived_at: i64 = 0;
    var resolution: ?[]const u8 = null;
    var resolution_note: ?[]const u8 = null;

    var seen_schema = false;
    var seen_id = false;
    var seen_project = false;
    var seen_kind = false;
    var seen_timestamp = false;
    var seen_created = false;
    var seen_title = false;
    var seen_tags = false;
    var seen_priority = false;
    var seen_archived = false;
    var seen_resolution = false;
    var seen_resolution_note = false;
    var unknown_keys: std.ArrayList([]const u8) = .empty;
    var closed = false;
    var body_start = content.len;
    var line_number: usize = 2;
    var pos = first_nl + 1;

    while (pos < content.len) : (line_number += 1) {
        const line_end = std.mem.indexOfScalarPos(u8, content, pos, '\n') orelse content.len;
        const next_pos = if (line_end < content.len) line_end + 1 else content.len;
        const line = std.mem.trim(u8, content[pos..line_end], " \r");
        pos = next_pos;

        if (std.mem.eql(u8, line, "---")) {
            closed = true;
            body_start = pos;
            break;
        }

        const colon_idx = std.mem.indexOfScalar(u8, line, ':') orelse continue;
        const key = std.mem.trim(u8, line[0..colon_idx], " ");
        const value = std.mem.trim(u8, line[colon_idx + 1 ..], " ");

        if (std.mem.eql(u8, key, "schema")) {
            try note_field(arena, issues, filename, line_number, key, &seen_schema);
            schema = std.fmt.parseInt(u16, value, 10) catch blk: {
                try add_issue(arena, issues, filename, line_number, "invalid_integer", .err, key, false, "Invalid schema value '{s}'", .{value});
                break :blk 0;
            };
            if (schema > 1) try add_issue(arena, issues, filename, line_number, "unsupported_schema", .err, key, false, "Unsupported schema version {d}", .{schema});
        } else if (std.mem.eql(u8, key, "id")) {
            try note_field(arena, issues, filename, line_number, key, &seen_id);
            id = try unescape_yaml_string(arena, value);
            if (id.?.len > 0 and !valid_id(id.?)) try add_issue(arena, issues, filename, line_number, "invalid_id", .err, key, false, "ID must be 12 hexadecimal characters", .{});
        } else if (std.mem.eql(u8, key, "project")) {
            try note_field(arena, issues, filename, line_number, key, &seen_project);
            project = try unescape_yaml_string(arena, value);
        } else if (std.mem.eql(u8, key, "kind")) {
            try note_field(arena, issues, filename, line_number, key, &seen_kind);
            const raw = try unescape_yaml_string(arena, value);
            if (canonical_kind(raw)) |canonical| {
                kind = canonical;
                if (!std.mem.eql(u8, raw, canonical)) try add_issue(arena, issues, filename, line_number, "noncanonical_value", .warning, key, true, "Normalize kind '{s}' to '{s}'", .{ raw, canonical });
            } else {
                kind = "unspecified";
                try add_issue(arena, issues, filename, line_number, "invalid_kind", .err, key, false, "Invalid kind '{s}'", .{raw});
            }
        } else if (std.mem.eql(u8, key, "title")) {
            try note_field(arena, issues, filename, line_number, key, &seen_title);
            title = try unescape_yaml_string(arena, value);
        } else if (std.mem.eql(u8, key, "timestamp")) {
            try note_field(arena, issues, filename, line_number, key, &seen_timestamp);
            timestamp = std.fmt.parseInt(i64, value, 10) catch blk: {
                try add_issue(arena, issues, filename, line_number, "invalid_integer", .err, key, false, "Invalid timestamp '{s}'", .{value});
                break :blk 0;
            };
        } else if (std.mem.eql(u8, key, "created_at_ns")) {
            try note_field(arena, issues, filename, line_number, key, &seen_created);
            created_at_ns = std.fmt.parseInt(i128, value, 10) catch blk: {
                try add_issue(arena, issues, filename, line_number, "invalid_integer", .err, key, false, "Invalid created_at_ns '{s}'", .{value});
                break :blk 0;
            };
        } else if (std.mem.eql(u8, key, "tags")) {
            try note_field(arena, issues, filename, line_number, key, &seen_tags);
            tags = try unescape_yaml_string(arena, value);
        } else if (std.mem.eql(u8, key, "priority")) {
            try note_field(arena, issues, filename, line_number, key, &seen_priority);
            const raw = try unescape_yaml_string(arena, value);
            if (canonical_priority(raw)) |canonical| {
                priority = canonical;
                if (!std.mem.eql(u8, raw, canonical)) try add_issue(arena, issues, filename, line_number, "noncanonical_value", .warning, key, true, "Normalize priority '{s}' to '{s}'", .{ raw, canonical });
            } else {
                priority = raw;
                try add_issue(arena, issues, filename, line_number, "invalid_priority", .err, key, false, "Invalid priority '{s}'", .{raw});
            }
        } else if (std.mem.eql(u8, key, "archived_at")) {
            try note_field(arena, issues, filename, line_number, key, &seen_archived);
            archived_at = std.fmt.parseInt(i64, value, 10) catch blk: {
                try add_issue(arena, issues, filename, line_number, "invalid_integer", .err, key, false, "Invalid archived_at '{s}'", .{value});
                break :blk 0;
            };
        } else if (std.mem.eql(u8, key, "resolution")) {
            try note_field(arena, issues, filename, line_number, key, &seen_resolution);
            resolution = try unescape_yaml_string(arena, value);
            if (resolution.?.len > 0 and !valid_resolution(resolution.?)) try add_issue(arena, issues, filename, line_number, "invalid_resolution", .err, key, false, "Invalid resolution '{s}'", .{resolution.?});
        } else if (std.mem.eql(u8, key, "resolution_note")) {
            try note_field(arena, issues, filename, line_number, key, &seen_resolution_note);
            resolution_note = try unescape_yaml_string(arena, value);
        } else {
            var duplicate = false;
            for (unknown_keys.items) |seen_key| if (std.mem.eql(u8, seen_key, key)) {
                duplicate = true;
                break;
            };
            if (duplicate) {
                try add_issue(arena, issues, filename, line_number, "duplicate_field", .err, key, false, "Duplicate front-matter field '{s}'", .{key});
            } else {
                try unknown_keys.append(arena, try arena.dupe(u8, key));
            }
        }
    }

    if (!closed) try add_issue(arena, issues, filename, line_number, "unterminated_front_matter", .err, "", false, "Missing closing front-matter delimiter", .{});
    if (!seen_schema) try add_issue(arena, issues, filename, 1, "missing_schema", .warning, "schema", true, "Legacy file has no schema version", .{});
    if (!seen_id) try add_issue(arena, issues, filename, 1, "missing_id", .warning, "id", true, "Legacy file has no explicit ID", .{});
    if (!seen_kind) try add_issue(arena, issues, filename, 1, "missing_kind", .info, "kind", false, "Legacy file has no kind", .{});
    if (project == null or project.?.len == 0) try add_issue(arena, issues, filename, 1, "missing_required_field", .err, "project", false, "Missing required field 'project'", .{});
    if (title == null or title.?.len == 0) try add_issue(arena, issues, filename, 1, "missing_required_field", .err, "title", false, "Missing required field 'title'", .{});

    return .{
        .schema = schema,
        .has_schema = seen_schema,
        .has_id = seen_id,
        .has_kind = seen_kind,
        .id = id orelse "",
        .filename = "",
        .project = project orelse "",
        .kind = kind orelse "unspecified",
        .timestamp = timestamp orelse 0,
        .created_at_ns = created_at_ns orelse 0,
        .title = title orelse "",
        .tags = tags orelse "",
        .priority = priority orelse "",
        .archived_at = archived_at,
        .resolution = resolution orelse "",
        .resolution_note = resolution_note orelse "",
        .body = if (closed) content[body_start..] else "",
        .score = 0,
    };
}

fn parse_front_matter_from_buf(arena: std.mem.Allocator, content: []const u8) !?IdeaMeta {
    var issues: std.ArrayList(Issue) = .empty;
    const meta = try parse_front_matter_detailed(arena, content, "", &issues);
    for (issues.items) |issue| if (issue.severity == .err) return null;
    return meta;
}

const FieldUpdate = struct {
    key: []const u8,
    value: ?[]const u8,
};

fn yaml_quoted(arena: std.mem.Allocator, value: []const u8) ![]const u8 {
    return std.fmt.allocPrint(arena, "\"{s}\"", .{try escape_yaml_string(arena, value)});
}

fn append_field_line(arena: std.mem.Allocator, output: *std.ArrayList(u8), key: []const u8, value: []const u8) !void {
    try output.appendSlice(arena, try std.fmt.allocPrint(arena, "{s}: {s}\n", .{ key, value }));
}

fn rewrite_front_matter(arena: std.mem.Allocator, content: []const u8, updates: []const FieldUpdate) ![]const u8 {
    const first_nl = std.mem.indexOfScalar(u8, content, '\n') orelse return error.InvalidFrontMatter;
    if (!std.mem.eql(u8, std.mem.trim(u8, content[0..first_nl], " \r"), "---")) return error.InvalidFrontMatter;

    const applied = try arena.alloc(bool, updates.len);
    @memset(applied, false);
    var output: std.ArrayList(u8) = .empty;
    try output.appendSlice(arena, content[0 .. first_nl + 1]);

    var pos = first_nl + 1;
    while (pos < content.len) {
        const line_start = pos;
        const line_end = std.mem.indexOfScalarPos(u8, content, pos, '\n') orelse content.len;
        const next_pos = if (line_end < content.len) line_end + 1 else content.len;
        const line = std.mem.trim(u8, content[line_start..line_end], " \r");
        pos = next_pos;

        if (std.mem.eql(u8, line, "---")) {
            for (updates, 0..) |update, i| {
                if (!applied[i]) if (update.value) |value| try append_field_line(arena, &output, update.key, value);
            }
            try output.appendSlice(arena, content[line_start..]);
            return output.items;
        }

        const colon_idx = std.mem.indexOfScalar(u8, line, ':');
        var matched = false;
        if (colon_idx) |index| {
            const key = std.mem.trim(u8, line[0..index], " ");
            for (updates, 0..) |update, i| {
                if (std.mem.eql(u8, key, update.key)) {
                    matched = true;
                    if (!applied[i]) {
                        if (update.value) |value| try append_field_line(arena, &output, update.key, value);
                        applied[i] = true;
                    }
                    break;
                }
            }
        }
        if (!matched) try output.appendSlice(arena, content[line_start..next_pos]);
    }
    return error.InvalidFrontMatter;
}

fn atomic_write(arena: std.mem.Allocator, io: std.Io, dir: std.Io.Dir, filename: []const u8, content: []const u8) !void {
    const nonce = std.Io.Timestamp.now(io, .real).toNanoseconds();
    const temp_name = try std.fmt.allocPrint(arena, ".{s}.{d}.tmp", .{ filename, nonce });
    dir.writeFile(io, .{ .sub_path = temp_name, .data = content }) catch |err| return err;
    std.Io.Dir.rename(dir, temp_name, dir, filename, io) catch |err| {
        dir.deleteFile(io, temp_name) catch {};
        return err;
    };
}

const VaultScan = struct {
    ideas: []IdeaMeta,
    issues: []Issue,
    files_scanned: usize,
};

fn scan_vault(arena: std.mem.Allocator, io: std.Io, dir: std.Io.Dir) !VaultScan {
    var ideas: std.ArrayList(IdeaMeta) = .empty;
    var issues: std.ArrayList(Issue) = .empty;
    var files_scanned: usize = 0;
    var it = dir.iterate();
    while (try it.next(io)) |entry| {
        if (entry.kind != .file or !std.mem.endsWith(u8, entry.name, ".md")) continue;
        files_scanned += 1;
        const content = dir.readFileAlloc(io, entry.name, arena, .unlimited) catch |err| {
            try add_issue(arena, &issues, entry.name, 0, "unreadable_file", .err, "", false, "Failed to read file: {any}", .{err});
            continue;
        };
        const issue_start = issues.items.len;
        if (try parse_front_matter_detailed(arena, content, entry.name, &issues)) |raw_meta| {
            var valid = true;
            for (issues.items[issue_start..]) |issue| if (issue.severity == .err) {
                valid = false;
                break;
            };
            var meta = raw_meta;
            meta.filename = try arena.dupe(u8, entry.name);
            meta.id = if (raw_meta.id.len > 0) raw_meta.id else try derive_id(arena, entry.name);
            if (raw_meta.has_id) {
                const stem = entry.name[0 .. entry.name.len - 3];
                const legacy_id = try derive_id(arena, entry.name);
                if (!std.mem.eql(u8, stem, raw_meta.id) and !std.mem.eql(u8, legacy_id, raw_meta.id)) try add_issue(arena, &issues, entry.name, 1, "id_filename_mismatch", .warning, "id", false, "ID '{s}' does not match filename stem '{s}'", .{ raw_meta.id, stem });
            }
            if (valid) try ideas.append(arena, meta);
        }
    }

    for (ideas.items, 0..) |idea, i| {
        for (ideas.items[i + 1 ..]) |other| {
            if (std.mem.eql(u8, idea.id, other.id)) try add_issue(arena, &issues, other.filename, 1, "duplicate_id", .err, "id", false, "ID '{s}' is also used by '{s}'", .{ other.id, idea.filename });
        }
    }
    return .{ .ideas = ideas.items, .issues = issues.items, .files_scanned = files_scanned };
}

fn repair_vault(arena: std.mem.Allocator, io: std.Io, dir: std.Io.Dir) !usize {
    var repaired: usize = 0;
    var it = dir.iterate();
    while (try it.next(io)) |entry| {
        if (entry.kind != .file or !std.mem.endsWith(u8, entry.name, ".md")) continue;
        const content = dir.readFileAlloc(io, entry.name, arena, .unlimited) catch continue;
        var issues: std.ArrayList(Issue) = .empty;
        const raw_meta = (try parse_front_matter_detailed(arena, content, entry.name, &issues)) orelse continue;
        var updates: std.ArrayList(FieldUpdate) = .empty;
        if (!raw_meta.has_schema) try updates.append(arena, .{ .key = "schema", .value = "1" });
        if (!raw_meta.has_id) {
            const derived = try derive_id(arena, entry.name);
            try updates.append(arena, .{ .key = "id", .value = try yaml_quoted(arena, derived) });
        }
        for (issues.items) |issue| {
            if (!std.mem.eql(u8, issue.code, "noncanonical_value")) continue;
            if (std.mem.eql(u8, issue.field, "kind")) try updates.append(arena, .{ .key = "kind", .value = try yaml_quoted(arena, raw_meta.kind) });
            if (std.mem.eql(u8, issue.field, "priority")) try updates.append(arena, .{ .key = "priority", .value = try yaml_quoted(arena, raw_meta.priority) });
        }
        if (updates.items.len == 0) continue;
        const rewritten = rewrite_front_matter(arena, content, updates.items) catch continue;
        try atomic_write(arena, io, dir, entry.name, rewritten);
        repaired += 1;
    }
    return repaired;
}

fn severity_name(severity: IssueSeverity) []const u8 {
    return switch (severity) {
        .info => "info",
        .warning => "warning",
        .err => "error",
    };
}

fn scan_has_failures(scan: VaultScan, strict: bool) bool {
    for (scan.issues) |issue| {
        if (issue.severity == .err or (strict and issue.severity == .warning)) return true;
    }
    return false;
}

fn emit_doctor_report(io: std.Io, vault_path: []const u8, scan: VaultScan, repaired: usize, format: OutputFormat) !void {
    var errors: usize = 0;
    var warnings: usize = 0;
    var infos: usize = 0;
    var repairable: usize = 0;
    for (scan.issues) |issue| {
        switch (issue.severity) {
            .err => errors += 1,
            .warning => warnings += 1,
            .info => infos += 1,
        }
        if (issue.repairable) repairable += 1;
    }

    var buf: [8192]u8 = undefined;
    var w = std.Io.File.stdout().writer(io, &buf);
    if (format == .json) {
        try w.interface.print("{{\"healthy\":{s},\"vault\":\"", .{if (errors == 0) "true" else "false"});
        try print_escaped_json(&w.interface, vault_path);
        try w.interface.print("\",\"files_scanned\":{d},\"repaired\":{d},\"issues\":[", .{ scan.files_scanned, repaired });
        for (scan.issues, 0..) |issue, i| {
            if (i > 0) try w.interface.writeAll(",");
            try w.interface.writeAll("{\"code\":\"");
            try print_escaped_json(&w.interface, issue.code);
            try w.interface.writeAll("\",\"severity\":\"");
            try w.interface.writeAll(severity_name(issue.severity));
            try w.interface.writeAll("\",\"filename\":\"");
            try print_escaped_json(&w.interface, issue.filename);
            try w.interface.print("\",\"line\":{d}", .{issue.line});
            if (issue.field.len > 0) {
                try w.interface.writeAll(",\"field\":\"");
                try print_escaped_json(&w.interface, issue.field);
                try w.interface.writeAll("\"");
            }
            try w.interface.writeAll(",\"message\":\"");
            try print_escaped_json(&w.interface, issue.message);
            try w.interface.print("\",\"repairable\":{s}}}", .{if (issue.repairable) "true" else "false"});
        }
        try w.interface.print("],\"summary\":{{\"errors\":{d},\"warnings\":{d},\"info\":{d},\"repairable\":{d}}}}}\n", .{ errors, warnings, infos, repairable });
    } else {
        try w.interface.print("Vault: {s}\nScanned: {d} Markdown file(s)\n", .{ vault_path, scan.files_scanned });
        if (repaired > 0) try w.interface.print("Repaired: {d} file(s)\n", .{repaired});
        for (scan.issues) |issue| {
            try w.interface.print("{s}: {s}", .{ severity_name(issue.severity), issue.filename });
            if (issue.line > 0) try w.interface.print(":{d}", .{issue.line});
            try w.interface.print(": {s} [{s}]", .{ issue.message, issue.code });
            if (issue.repairable) try w.interface.writeAll(" (repairable)");
            try w.interface.writeAll("\n");
        }
        if (scan.issues.len == 0) try w.interface.writeAll("No integrity issues found.\n");
        try w.interface.print("Summary: {d} error(s), {d} warning(s), {d} info\n", .{ errors, warnings, infos });
    }
    try w.end();
}

fn strip_yaml_quotes(value: []const u8) []const u8 {
    if (value.len >= 2 and value[0] == '"' and value[value.len - 1] == '"') {
        return value[1 .. value.len - 1];
    } else if (value.len >= 2 and value[0] == '\'' and value[value.len - 1] == '\'') {
        return value[1 .. value.len - 1];
    }
    return value;
}

fn validate_filename(filename: []const u8) bool {
    if (filename.len == 0) return false;
    if (std.mem.indexOf(u8, filename, "..") != null) return false;
    if (std.mem.indexOfScalar(u8, filename, '/') != null) return false;
    if (std.mem.indexOfScalar(u8, filename, '\\') != null) return false;
    if (filename[0] == '.') return false;
    return true;
}

fn get_cwd(arena: std.mem.Allocator, io: std.Io) ![]const u8 {
    var buf: [std.fs.max_path_bytes]u8 = undefined;
    const len = try std.process.currentPath(io, &buf);
    return arena.dupe(u8, buf[0..len]);
}

fn find_repo_root(arena: std.mem.Allocator, io: std.Io, cwd: []const u8) !?[]const u8 {
    var current = cwd;
    while (true) {
        const marker = try std.fs.path.join(arena, &.{ current, ".git" });
        if (std.Io.Dir.accessAbsolute(io, marker, .{})) |_| {
            return try arena.dupe(u8, current);
        } else |err| switch (err) {
            error.FileNotFound => {},
            else => return err,
        }

        const parent = std.fs.path.dirname(current) orelse break;
        if (std.mem.eql(u8, parent, current)) break;
        current = parent;
    }
    return null;
}

fn resolve_project(arena: std.mem.Allocator, io: std.Io, environ_map: anytype, override: ?[]const u8) ![]const u8 {
    if (override) |project| return project;
    if (environ_map.get("PIN_PROJECT")) |project| {
        if (project.len > 0) return project;
    }

    const cwd = try get_cwd(arena, io);
    const root = (try find_repo_root(arena, io, cwd)) orelse cwd;
    const config_path = try std.fs.path.join(arena, &.{ root, ".pin-project" });
    const configured = std.Io.Dir.cwd().readFileAlloc(io, config_path, arena, .limited(4096)) catch |err| switch (err) {
        error.FileNotFound => null,
        else => return err,
    };
    if (configured) |raw| {
        const project = std.mem.trim(u8, raw, " \t\r\n");
        if (project.len > 0) return project;
    }
    return std.fs.path.basename(root);
}

fn resolve_vault_path(arena: std.mem.Allocator, io: std.Io, environ_map: anytype) []const u8 {
    if (environ_map.get("PIN_VAULT")) |custom| {
        if (custom.len > 0) {
            if (std.fs.path.isAbsolute(custom)) return custom;
            const base = get_cwd(arena, io) catch {
                std.debug.print("Error: Failed to resolve relative PIN_VAULT.\n", .{});
                std.process.exit(1);
            };
            return std.fs.path.join(arena, &.{ base, custom }) catch {
                std.debug.print("Error: Failed to construct PIN_VAULT path.\n", .{});
                std.process.exit(1);
            };
        }
    }

    const cwd = get_cwd(arena, io) catch {
        std.debug.print("Error: Failed to get current directory.\n", .{});
        std.process.exit(1);
    };
    const root = (find_repo_root(arena, io, cwd) catch null) orelse cwd;
    const local_vault = std.fs.path.join(arena, &.{ root, ".pin_vault" }) catch {
        std.debug.print("Error: Failed to construct local vault path.\n", .{});
        std.process.exit(1);
    };
    if (std.Io.Dir.accessAbsolute(io, local_vault, .{})) |_| return local_vault else |_| {}

    const home = environ_map.get("HOME") orelse {
        std.debug.print("Error: HOME environment variable is not set.\n", .{});
        std.process.exit(1);
    };
    return std.fs.path.join(arena, &.{ home, ".pin_vault" }) catch {
        std.debug.print("Error: Failed to construct vault path.\n", .{});
        std.process.exit(1);
    };
}

fn parse_format(value: []const u8) ?OutputFormat {
    if (std.mem.eql(u8, value, "json")) return .json;
    if (std.mem.eql(u8, value, "table")) return .table;
    if (std.mem.eql(u8, value, "plain")) return .plain;
    return null;
}

fn default_format(io: std.Io, human_format: OutputFormat) OutputFormat {
    return if (std.Io.File.stdout().isTty(io) catch false) human_format else .json;
}

fn valid_priority(priority: []const u8) bool {
    return std.mem.eql(u8, priority, "low") or std.mem.eql(u8, priority, "medium") or std.mem.eql(u8, priority, "high");
}

fn valid_kind(kind: []const u8) bool {
    return std.mem.eql(u8, kind, "technical") or
        std.mem.eql(u8, kind, "product") or
        std.mem.eql(u8, kind, "business") or
        std.mem.eql(u8, kind, "project") or
        std.mem.eql(u8, kind, "unspecified");
}

fn kind_label(kind: []const u8) []const u8 {
    if (std.mem.eql(u8, kind, "technical")) return "Technical";
    if (std.mem.eql(u8, kind, "product")) return "Product";
    if (std.mem.eql(u8, kind, "business")) return "Business";
    if (std.mem.eql(u8, kind, "project")) return "Project";
    return "Unspecified";
}

fn kind_index(kind: []const u8) usize {
    if (std.mem.eql(u8, kind, "technical")) return 0;
    if (std.mem.eql(u8, kind, "product")) return 1;
    if (std.mem.eql(u8, kind, "business")) return 2;
    if (std.mem.eql(u8, kind, "project")) return 3;
    return 4;
}

const kind_names = [_][]const u8{ "technical", "product", "business", "project", "unspecified" };

fn open_vault_dir(io: std.Io, vault_path: []const u8, iterate: bool) ?std.Io.Dir {
    return std.Io.Dir.openDirAbsolute(io, vault_path, .{ .iterate = iterate }) catch |err| {
        if (err == error.FileNotFound) return null;
        std.debug.print("Error: Failed to open vault directory: {any}\n", .{err});
        std.process.exit(1);
    };
}

fn is_word_byte(c: u8) bool {
    return std.ascii.isAlphanumeric(c) or c == '_';
}

fn contains_word_ignore_case(haystack: []const u8, needle: []const u8) bool {
    if (needle.len == 0 or needle.len > haystack.len) return false;
    var pos: usize = 0;
    while (std.ascii.findIgnoreCase(haystack[pos..], needle)) |relative| {
        const start = pos + relative;
        const end = start + needle.len;
        if ((start == 0 or !is_word_byte(haystack[start - 1])) and (end == haystack.len or !is_word_byte(haystack[end]))) return true;
        pos = start + 1;
        if (pos + needle.len > haystack.len) break;
    }
    return false;
}

fn search_score(meta: IdeaMeta, query: []const u8) ?usize {
    var score: usize = 0;
    var terms = std.mem.tokenizeAny(u8, query, " \t\r\n");
    var term_count: usize = 0;
    while (terms.next()) |term| {
        term_count += 1;
        var matched = false;
        if (std.ascii.findIgnoreCase(meta.title, term) != null) {
            score += 8;
            matched = true;
            if (contains_word_ignore_case(meta.title, term)) score += 2;
        }
        if (std.ascii.findIgnoreCase(meta.tags, term) != null) {
            score += 4;
            matched = true;
            if (contains_word_ignore_case(meta.tags, term)) score += 1;
        }
        if (std.ascii.findIgnoreCase(meta.body, term) != null) {
            score += 1;
            matched = true;
            if (contains_word_ignore_case(meta.body, term)) score += 1;
        }
        if (!matched) return null;
    }
    if (term_count == 0) return 1;
    if (std.ascii.eqlIgnoreCase(std.mem.trim(u8, meta.title, " \t\r\n"), std.mem.trim(u8, query, " \t\r\n"))) score += 32 else if (std.ascii.findIgnoreCase(meta.title, query) != null) score += 16;
    if (std.ascii.findIgnoreCase(meta.body, query) != null) score += 2;
    return score;
}

fn archive_matches(meta: IdeaMeta, filter: ArchiveFilter) bool {
    return switch (filter) {
        .active => meta.archived_at == 0,
        .archived => meta.archived_at > 0,
        .all => true,
    };
}

fn collect_ideas_filtered(
    arena: std.mem.Allocator,
    io: std.Io,
    dir: std.Io.Dir,
    filter_project: ?[]const u8,
    filter_query: ?[]const u8,
    filter_tag: ?[]const u8,
    filter_kind: ?[]const u8,
    archive_filter: ArchiveFilter,
) ![]IdeaMeta {
    var list: std.ArrayList(IdeaMeta) = .empty;
    var it = dir.iterate();

    while (try it.next(io)) |entry| {
        if (entry.kind != .file or !std.mem.endsWith(u8, entry.name, ".md")) continue;
        const content = dir.readFileAlloc(io, entry.name, arena, .unlimited) catch continue;
        const meta_opt = try parse_front_matter_from_buf(arena, content);
        if (meta_opt) |raw_meta| {
            var meta = raw_meta;
            meta.id = if (raw_meta.id.len > 0) raw_meta.id else try derive_id(arena, entry.name);
            meta.filename = try arena.dupe(u8, entry.name);

            if (!archive_matches(meta, archive_filter)) continue;
            if (filter_project) |proj| if (!std.mem.eql(u8, meta.project, proj)) continue;
            if (filter_tag) |tag| if (!tag_matches(meta.tags, tag)) continue;
            if (filter_kind) |kind| if (!std.ascii.eqlIgnoreCase(meta.kind, kind)) continue;
            if (filter_query) |query| meta.score = search_score(meta, query) orelse continue;
            try list.append(arena, meta);
        }
    }

    const items = list.items;
    if (filter_query != null) {
        std.sort.block(IdeaMeta, items, {}, IdeaMeta.searchLessThan);
    } else {
        std.sort.block(IdeaMeta, items, {}, IdeaMeta.lessThan);
    }
    return items;
}

fn collect_ideas(
    arena: std.mem.Allocator,
    io: std.Io,
    dir: std.Io.Dir,
    filter_project: ?[]const u8,
    filter_query: ?[]const u8,
    filter_tag: ?[]const u8,
    filter_kind: ?[]const u8,
) ![]IdeaMeta {
    return collect_ideas_filtered(arena, io, dir, filter_project, filter_query, filter_tag, filter_kind, .active);
}

fn tag_matches(tags_csv: []const u8, needle: []const u8) bool {
    if (tags_csv.len == 0) return false;
    var pos: usize = 0;
    while (pos < tags_csv.len) {
        const end = std.mem.indexOfScalarPos(u8, tags_csv, pos, ',') orelse tags_csv.len;
        const tag = std.mem.trim(u8, tags_csv[pos..end], " ");
        if (std.ascii.eqlIgnoreCase(tag, needle)) return true;
        if (end >= tags_csv.len) break;
        pos = end + 1;
    }
    return false;
}

const SelectorError = error{ SelectorNotFound, AmbiguousSelector };

fn resolve_selector(arena: std.mem.Allocator, io: std.Io, dir: std.Io.Dir, selector: []const u8) ![]const u8 {
    if (validate_filename(selector) and std.mem.endsWith(u8, selector, ".md")) {
        if (dir.statFile(io, selector, .{})) |_| {
            return try arena.dupe(u8, selector);
        } else |err| switch (err) {
            error.FileNotFound => {},
            else => return err,
        }
    }

    const ideas = try collect_ideas_filtered(arena, io, dir, null, null, null, null, .all);
    var match: ?[]const u8 = null;
    for (ideas) |meta| {
        if (std.mem.eql(u8, meta.id, selector)) return meta.filename;
        if (std.mem.startsWith(u8, meta.id, selector)) {
            if (match != null) return SelectorError.AmbiguousSelector;
            match = meta.filename;
        }
    }
    return match orelse SelectorError.SelectorNotFound;
}

fn read_stdin(arena: std.mem.Allocator, io: std.Io) !?[]const u8 {
    const stdin_file = std.Io.File.stdin();

    // Check if stdin is a pipe (not a terminal)
    if (stdin_file.isTty(io) catch return null) return null;

    var buf: [4096]u8 = undefined;
    var r = stdin_file.reader(io, &buf);
    const content = r.interface.allocRemaining(arena, .unlimited) catch return null;

    if (content.len == 0) return null;
    return content;
}

fn format_date(writer: *std.Io.Writer, timestamp: i64) !void {
    const epoch_secs = std.time.epoch.EpochSeconds{ .secs = @intCast(timestamp) };
    const epoch_day = epoch_secs.getEpochDay();
    const year_day = epoch_day.calculateYearDay();
    const month_day = year_day.calculateMonthDay();
    try writer.print("{d:0>4}-{d:0>2}-{d:0>2}", .{ year_day.year, @intFromEnum(month_day.month), month_day.day_index + 1 });
}

fn require_flag_value(args: []const []const u8, idx: *usize, flag: []const u8) []const u8 {
    if (idx.* + 1 >= args.len) {
        std.debug.print("Error: {s} requires a value\n", .{flag});
        std.process.exit(1);
    }
    idx.* += 1;
    return args[idx.*];
}

fn parse_format_arg(value: []const u8, allow_table: bool, command: []const u8) OutputFormat {
    const format = parse_format(value) orelse {
        if (allow_table) {
            std.debug.print("Error: --format must be json, table, or plain for '{s}'\n", .{command});
        } else {
            std.debug.print("Error: --format must be json or plain for '{s}'\n", .{command});
        }
        std.process.exit(1);
    };
    if (!allow_table and format == .table) {
        std.debug.print("Error: --format must be json or plain for '{s}'\n", .{command});
        std.process.exit(1);
    }
    return format;
}

const SelectorArgs = struct {
    selector: []const u8,
    format: ?OutputFormat,
};

fn parse_selector_args(args: []const []const u8, command: []const u8) SelectorArgs {
    if (args.len < 3) {
        std.debug.print("Error: '{s}' requires an ID, ID prefix, or filename\n", .{command});
        std.process.exit(1);
    }
    var result = SelectorArgs{ .selector = args[2], .format = null };
    var idx: usize = 3;
    while (idx < args.len) : (idx += 1) {
        if (!std.mem.eql(u8, args[idx], "--format")) {
            std.debug.print("Error: Unexpected argument '{s}'\n", .{args[idx]});
            std.process.exit(1);
        }
        result.format = parse_format_arg(require_flag_value(args, &idx, "--format"), false, command);
    }
    if (!validate_filename(result.selector)) {
        std.debug.print("Error: Invalid selector '{s}'\n", .{result.selector});
        std.process.exit(1);
    }
    return result;
}

fn open_vault_or_exit(io: std.Io, vault_path: []const u8) std.Io.Dir {
    return std.Io.Dir.openDirAbsolute(io, vault_path, .{ .iterate = true }) catch |err| {
        if (err == error.FileNotFound) std.debug.print("Error: Vault not found.\n", .{}) else std.debug.print("Error: Failed to open vault directory: {any}\n", .{err});
        std.process.exit(1);
    };
}

fn resolve_selector_or_exit(arena: std.mem.Allocator, io: std.Io, dir: std.Io.Dir, selector: []const u8) []const u8 {
    return resolve_selector(arena, io, dir, selector) catch |err| {
        if (err == SelectorError.AmbiguousSelector) {
            std.debug.print("Error: Selector '{s}' is ambiguous. Use more ID characters.\n", .{selector});
        } else if (err == SelectorError.SelectorNotFound) {
            std.debug.print("Error: No idea matches '{s}'.\n", .{selector});
        } else {
            std.debug.print("Error: Failed to resolve '{s}': {any}\n", .{ selector, err });
        }
        std.process.exit(1);
    };
}

fn idea_id_for_file(arena: std.mem.Allocator, io: std.Io, dir: std.Io.Dir, filename: []const u8) ![]const u8 {
    const content = try dir.readFileAlloc(io, filename, arena, .unlimited);
    const meta = try parse_front_matter_from_buf(arena, content);
    if (meta) |value| if (value.id.len > 0) return value.id;
    return derive_id(arena, filename);
}

fn print_usage(io: std.Io) !void {
    var buf: [4096]u8 = undefined;
    var w = std.Io.File.stdout().writer(io, &buf);
    try w.interface.print(
        \\pin {s} — a small idea registry for humans and agents
        \\
        \\Usage:
        \\  pin init --local [--project <name>] [--format json|plain]
        \\  pin add "<markdown>" --kind technical|product|business|project [--project <name>] [--title <title>] [--tags <csv>] [--priority low|medium|high] [--format json|plain]
        \\  pin add --stdin [options]
        \\  pin list [--project <name>] [--tag <name>] [--kind <kind>] [--archived|--all] [--format json|table|plain]
        \\  pin list-project [--tag <name>] [--kind <kind>] [--archived|--all] [--format json|table|plain]
        \\  pin search "<query>" [--project <name>] [--tag <name>] [--kind <kind>] [--limit <n>] [--archived|--all] [--format json|table|plain]
        \\  pin context [--project <name>] [--kind <kind>] [--limit <n>] [--group kind] [--archived|--all] [--format json|plain]
        \\  pin doctor [--repair] [--strict] [--format json|plain]
        \\  pin archive <id|id-prefix|filename> [--resolution implemented|rejected|superseded|stale] [--note <text>] [--format json|plain]
        \\  pin unarchive <id|id-prefix|filename> [--format json|plain]
        \\  pin import <directory> [--force] [--format json|plain]
        \\  pin export <directory> [--force] [--format json|plain]
        \\  pin read <id|id-prefix|filename> [--format json|plain]
        \\  pin rm <id|id-prefix|filename> [--format json|plain]
        \\  pin edit <id|id-prefix|filename> [--format json|plain]
        \\  pin stats [--format json|plain]
        \\  pin help | --help | --version
        \\
        \\Environment:
        \\  PIN_VAULT      Override vault path (~/.pin_vault)
        \\  PIN_PROJECT    Override repository-aware project name
        \\
        \\A .pin-project file at the repository root can also set the project name.
        \\
    , .{version});
    try w.end();
}

pub fn main(init: std.process.Init) !void {
    const io = init.io;
    const arena = init.arena.allocator();

    const args = try init.minimal.args.toSlice(arena);
    if (args.len < 2) {
        try print_usage(io);
        std.process.exit(1);
    }

    const cmd = args[1];
    if (std.mem.eql(u8, cmd, "--help") or std.mem.eql(u8, cmd, "-h") or std.mem.eql(u8, cmd, "help")) {
        try print_usage(io);
        return;
    }
    if (std.mem.eql(u8, cmd, "--version") or std.mem.eql(u8, cmd, "-V")) {
        var buf: [64]u8 = undefined;
        var writer = std.Io.File.stdout().writer(io, &buf);
        try writer.interface.print("pin {s}\n", .{version});
        try writer.end();
        return;
    }
    if (args.len > 2 and (std.mem.eql(u8, args[2], "--help") or std.mem.eql(u8, args[2], "-h"))) {
        try print_usage(io);
        return;
    }

    const vault_path = resolve_vault_path(arena, io, init.environ_map);

    // ── init ─────────────────────────────────────────────────────────────
    if (std.mem.eql(u8, cmd, "init")) {
        var local = false;
        var project: ?[]const u8 = null;
        var format: ?OutputFormat = null;
        var idx: usize = 2;
        while (idx < args.len) : (idx += 1) {
            if (std.mem.eql(u8, args[idx], "--local")) {
                local = true;
            } else if (std.mem.eql(u8, args[idx], "--project") or std.mem.eql(u8, args[idx], "--format")) {
                const flag = args[idx];
                const value = require_flag_value(args, &idx, flag);
                if (std.mem.eql(u8, flag, "--project")) project = value else format = parse_format_arg(value, false, cmd);
            } else {
                std.debug.print("Error: Unknown flag '{s}'\n", .{args[idx]});
                std.process.exit(1);
            }
        }
        if (!local) {
            std.debug.print("Error: 'init' currently requires --local\n", .{});
            std.process.exit(1);
        }
        const cwd = try get_cwd(arena, io);
        const root = (try find_repo_root(arena, io, cwd)) orelse cwd;
        const local_path = try std.fs.path.join(arena, &.{ root, ".pin_vault" });
        try std.Io.Dir.cwd().createDirPath(io, local_path);
        var local_dir = try std.Io.Dir.openDirAbsolute(io, local_path, .{});
        defer local_dir.close(io);
        try local_dir.writeFile(io, .{ .sub_path = ".gitkeep", .data = "" });
        const config_path = try std.fs.path.join(arena, &.{ root, ".pin-project" });
        if (std.Io.Dir.accessAbsolute(io, config_path, .{})) |_| {} else |_| {
            const project_name = project orelse std.fs.path.basename(root);
            const config = try std.fmt.allocPrint(arena, "{s}\n", .{project_name});
            try std.Io.Dir.cwd().writeFile(io, .{ .sub_path = config_path, .data = config });
        }
        var out_buf: [std.fs.max_path_bytes + 128]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        if ((format orelse default_format(io, .plain)) == .json) {
            try w.interface.writeAll("{\"vault\":\"");
            try print_escaped_json(&w.interface, local_path);
            try w.interface.writeAll("\",\"scope\":\"local\"}\n");
        } else try w.interface.print("Initialized local vault at {s}\n", .{local_path});
        try w.end();

        // ── add ──────────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "add")) {
        var content: ?[]const u8 = null;
        var project: ?[]const u8 = null;
        var title: ?[]const u8 = null;
        var tags: ?[]const u8 = null;
        var kind: ?[]const u8 = null;
        var priority: ?[]const u8 = null;
        var format: ?OutputFormat = null;
        var use_stdin = false;
        var allow_duplicate = false;

        var idx: usize = 2;
        while (idx < args.len) : (idx += 1) {
            const arg = args[idx];
            if (std.mem.eql(u8, arg, "--stdin")) {
                use_stdin = true;
            } else if (std.mem.eql(u8, arg, "--allow-duplicate")) {
                allow_duplicate = true;
            } else if (std.mem.eql(u8, arg, "--project") or std.mem.eql(u8, arg, "--title") or std.mem.eql(u8, arg, "--tags") or std.mem.eql(u8, arg, "--kind") or std.mem.eql(u8, arg, "--priority") or std.mem.eql(u8, arg, "--format")) {
                const value = require_flag_value(args, &idx, arg);
                if (std.mem.eql(u8, arg, "--project")) project = value else if (std.mem.eql(u8, arg, "--title")) title = value else if (std.mem.eql(u8, arg, "--tags")) tags = value else if (std.mem.eql(u8, arg, "--kind")) {
                    if (!valid_kind(value) or std.mem.eql(u8, value, "unspecified")) {
                        std.debug.print("Error: --kind must be technical, product, business, or project\n", .{});
                        std.process.exit(1);
                    }
                    kind = value;
                } else if (std.mem.eql(u8, arg, "--priority")) {
                    if (!valid_priority(value)) {
                        std.debug.print("Error: --priority must be low, medium, or high\n", .{});
                        std.process.exit(1);
                    }
                    priority = value;
                } else format = parse_format_arg(value, false, cmd);
            } else if (std.mem.startsWith(u8, arg, "--")) {
                std.debug.print("Error: Unknown flag '{s}'\n", .{arg});
                std.process.exit(1);
            } else if (content != null) {
                std.debug.print("Error: Multiple content arguments provided\n", .{});
                std.process.exit(1);
            } else {
                content = arg;
            }
        }

        if (use_stdin or content == null) {
            if (try read_stdin(arena, io)) |stdin_content| content = stdin_content;
        }
        const final_content = content orelse {
            std.debug.print("Error: Content argument is required for 'add'\n", .{});
            std.process.exit(1);
        };
        const kind_val = kind orelse {
            std.debug.print("Error: --kind is required (technical, product, business, or project)\n", .{});
            std.process.exit(1);
        };
        const proj_name = resolve_project(arena, io, init.environ_map, project) catch |err| {
            std.debug.print("Error: Failed to resolve project: {any}\n", .{err});
            std.process.exit(1);
        };
        const title_val = if (title) |t| try truncate_title(arena, t) else try get_default_title(arena, final_content);
        if (title_val.len == 0) {
            std.debug.print("Error: Could not determine a non-empty title\n", .{});
            std.process.exit(1);
        }

        std.Io.Dir.cwd().createDirPath(io, vault_path) catch |err| {
            std.debug.print("Error: Failed to create vault directory at '{s}': {any}\n", .{ vault_path, err });
            std.process.exit(1);
        };
        var dir = std.Io.Dir.openDirAbsolute(io, vault_path, .{ .iterate = true }) catch |err| {
            std.debug.print("Error: Failed to open vault directory: {any}\n", .{err});
            std.process.exit(1);
        };
        defer dir.close(io);

        if (!allow_duplicate) {
            const existing = try collect_ideas(arena, io, dir, proj_name, null, null, null);
            for (existing) |meta| {
                if (std.ascii.eqlIgnoreCase(meta.title, title_val)) {
                    std.debug.print("Error: An idea titled '{s}' already exists for project '{s}' ({s}). Use --allow-duplicate to add it anyway.\n", .{ title_val, proj_name, meta.id });
                    std.process.exit(1);
                }
            }
        }

        const ts = std.Io.Timestamp.now(io, .real);
        const seconds = ts.toSeconds();
        const total_ns = ts.toNanoseconds();
        const id_seed = try std.fmt.allocPrint(arena, "{d}:{s}:{s}", .{ total_ns, proj_name, title_val });
        const id = try derive_id(arena, id_seed);
        const filename = try std.fmt.allocPrint(arena, "{s}.md", .{id});
        if (dir.statFile(io, filename, .{})) |_| {
            std.debug.print("Error: Generated ID collision for '{s}'. Retry the command.\n", .{id});
            std.process.exit(1);
        } else |err| switch (err) {
            error.FileNotFound => {},
            else => return err,
        }
        const escaped_proj = try escape_yaml_string(arena, proj_name);
        const escaped_title = try escape_yaml_string(arena, title_val);
        const tags_line = if (tags) |value| try std.fmt.allocPrint(arena, "tags: \"{s}\"\n", .{try escape_yaml_string(arena, value)}) else "";
        const priority_line = if (priority) |value| try std.fmt.allocPrint(arena, "priority: \"{s}\"\n", .{value}) else "";
        const file_content = try std.fmt.allocPrint(arena,
            \\---
            \\schema: 1
            \\id: "{s}"
            \\project: "{s}"
            \\kind: "{s}"
            \\timestamp: {d}
            \\created_at_ns: {d}
            \\title: "{s}"
            \\{s}{s}---
            \\{s}
            \\
        , .{ id, escaped_proj, kind_val, seconds, total_ns, escaped_title, tags_line, priority_line, final_content });

        dir.writeFile(io, .{ .sub_path = filename, .data = file_content }) catch |err| {
            std.debug.print("Error: Failed to write file '{s}': {any}\n", .{ filename, err });
            std.process.exit(1);
        };

        const meta = IdeaMeta{ .schema = 1, .has_schema = true, .has_id = true, .has_kind = true, .id = id, .filename = filename, .project = proj_name, .kind = kind_val, .timestamp = seconds, .created_at_ns = total_ns, .title = title_val, .tags = tags orelse "", .priority = priority orelse "", .archived_at = 0, .resolution = "", .resolution_note = "", .body = final_content, .score = 0 };
        var out_buf: [1024]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        switch (format orelse default_format(io, .plain)) {
            .json => {
                try emit_idea_json(&w.interface, meta, true);
                try w.interface.writeAll("\n");
            },
            .plain => try w.interface.print("Saved {s}  {s}\n", .{ id, title_val }),
            .table => unreachable,
        }
        try w.end();

        // ── list ─────────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "list") or std.mem.eql(u8, cmd, "list-project")) {
        var filter_project: ?[]const u8 = null;
        var filter_tag: ?[]const u8 = null;
        var filter_kind: ?[]const u8 = null;
        var archive_filter: ArchiveFilter = .active;
        var format: ?OutputFormat = null;

        if (std.mem.eql(u8, cmd, "list-project")) {
            filter_project = resolve_project(arena, io, init.environ_map, null) catch |err| {
                std.debug.print("Error: Failed to resolve project: {any}\n", .{err});
                std.process.exit(1);
            };
        }

        var idx: usize = 2;
        while (idx < args.len) : (idx += 1) {
            const arg = args[idx];
            if (std.mem.eql(u8, arg, "--archived")) {
                archive_filter = .archived;
            } else if (std.mem.eql(u8, arg, "--all")) {
                archive_filter = .all;
            } else if (std.mem.eql(u8, arg, "--project") or std.mem.eql(u8, arg, "--tag") or std.mem.eql(u8, arg, "--kind") or std.mem.eql(u8, arg, "--format")) {
                const value = require_flag_value(args, &idx, arg);
                if (std.mem.eql(u8, arg, "--project")) filter_project = value else if (std.mem.eql(u8, arg, "--tag")) filter_tag = value else if (std.mem.eql(u8, arg, "--kind")) {
                    if (!valid_kind(value)) {
                        std.debug.print("Error: --kind must be technical, product, business, project, or unspecified\n", .{});
                        std.process.exit(1);
                    }
                    filter_kind = value;
                } else format = parse_format_arg(value, true, cmd);
            } else {
                std.debug.print("Error: Unknown flag '{s}'\n", .{arg});
                std.process.exit(1);
            }
        }
        const output_format = format orelse default_format(io, .table);

        var dir = open_vault_dir(io, vault_path, true) orelse {
            var out_buf: [32]u8 = undefined;
            var w = std.Io.File.stdout().writer(io, &out_buf);
            if (output_format == .json) try w.interface.writeAll("[]\n") else if (output_format == .table) try w.interface.writeAll("No ideas found.\n");
            try w.end();
            return;
        };
        defer dir.close(io);

        const ideas = try collect_ideas_filtered(arena, io, dir, filter_project, null, filter_tag, filter_kind, archive_filter);

        var out_buf: [4096]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);

        switch (output_format) {
            .table => if (ideas.len == 0) {
                try w.interface.writeAll("No ideas found.\n");
            } else {
                try w.interface.writeAll("DATE        PROJECT           KIND         ID            TITLE\n");
                try w.interface.writeAll("----------  ----------------  -----------  ------------  ----------------------------------------\n");
                for (ideas) |meta| try emit_idea_table(&w.interface, meta);
                try w.interface.print("\n{d} idea(s)\n", .{ideas.len});
            },
            .plain => for (ideas) |meta| try emit_idea_plain(&w.interface, meta),
            .json => {
                try w.interface.writeAll("[");
                for (ideas, 0..) |meta, i| try emit_idea_json(&w.interface, meta, i == 0);
                try w.interface.writeAll("]\n");
            },
        }
        try w.end();

        // ── search ───────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "search")) {
        if (args.len < 3) {
            std.debug.print("Error: 'search' subcommand requires a query argument\n", .{});
            std.process.exit(1);
        }
        const query = args[2];
        var filter_project: ?[]const u8 = null;
        var filter_tag: ?[]const u8 = null;
        var filter_kind: ?[]const u8 = null;
        var archive_filter: ArchiveFilter = .active;
        var limit: usize = std.math.maxInt(usize);
        var format: ?OutputFormat = null;

        var idx: usize = 3;
        while (idx < args.len) : (idx += 1) {
            const arg = args[idx];
            if (std.mem.eql(u8, arg, "--archived")) {
                archive_filter = .archived;
            } else if (std.mem.eql(u8, arg, "--all")) {
                archive_filter = .all;
            } else if (std.mem.eql(u8, arg, "--project") or std.mem.eql(u8, arg, "--tag") or std.mem.eql(u8, arg, "--kind") or std.mem.eql(u8, arg, "--limit") or std.mem.eql(u8, arg, "--format")) {
                const value = require_flag_value(args, &idx, arg);
                if (std.mem.eql(u8, arg, "--project")) filter_project = value else if (std.mem.eql(u8, arg, "--tag")) filter_tag = value else if (std.mem.eql(u8, arg, "--kind")) {
                    if (!valid_kind(value)) {
                        std.debug.print("Error: --kind must be technical, product, business, project, or unspecified\n", .{});
                        std.process.exit(1);
                    }
                    filter_kind = value;
                } else if (std.mem.eql(u8, arg, "--limit")) {
                    limit = std.fmt.parseInt(usize, value, 10) catch {
                        std.debug.print("Error: --limit must be a non-negative integer\n", .{});
                        std.process.exit(1);
                    };
                } else format = parse_format_arg(value, true, cmd);
            } else {
                std.debug.print("Error: Unknown flag '{s}'\n", .{arg});
                std.process.exit(1);
            }
        }
        const output_format = format orelse default_format(io, .table);

        var dir = open_vault_dir(io, vault_path, true) orelse {
            var out_buf: [32]u8 = undefined;
            var w = std.Io.File.stdout().writer(io, &out_buf);
            if (output_format == .json) try w.interface.writeAll("[]\n") else if (output_format == .table) try w.interface.writeAll("No ideas found.\n");
            try w.end();
            return;
        };
        defer dir.close(io);

        const ideas = try collect_ideas_filtered(arena, io, dir, filter_project, query, filter_tag, filter_kind, archive_filter);
        const selected = ideas[0..@min(limit, ideas.len)];

        var out_buf: [4096]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);

        switch (output_format) {
            .table => if (selected.len == 0) {
                try w.interface.print("No results for \"{s}\".\n", .{query});
            } else {
                try w.interface.writeAll("DATE        PROJECT           KIND         ID            TITLE\n");
                try w.interface.writeAll("----------  ----------------  -----------  ------------  ----------------------------------------\n");
                for (selected) |meta| try emit_idea_table(&w.interface, meta);
                try w.interface.print("\n{d} result(s) for \"{s}\"\n", .{ selected.len, query });
            },
            .plain => for (selected) |meta| try emit_idea_plain(&w.interface, meta),
            .json => {
                try w.interface.writeAll("[");
                for (selected, 0..) |meta, i| try emit_idea_json(&w.interface, meta, i == 0);
                try w.interface.writeAll("]\n");
            },
        }
        try w.end();

        // ── context ──────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "context")) {
        var filter_project: ?[]const u8 = null;
        var filter_kind: ?[]const u8 = null;
        var archive_filter: ArchiveFilter = .active;
        var limit: usize = 10;
        var group_kind = false;
        var format: ?OutputFormat = null;
        var idx: usize = 2;
        while (idx < args.len) : (idx += 1) {
            const arg = args[idx];
            if (std.mem.eql(u8, arg, "--archived")) {
                archive_filter = .archived;
            } else if (std.mem.eql(u8, arg, "--all")) {
                archive_filter = .all;
            } else if (std.mem.eql(u8, arg, "--project") or std.mem.eql(u8, arg, "--kind") or std.mem.eql(u8, arg, "--limit") or std.mem.eql(u8, arg, "--group") or std.mem.eql(u8, arg, "--format")) {
                const value = require_flag_value(args, &idx, arg);
                if (std.mem.eql(u8, arg, "--project")) filter_project = value else if (std.mem.eql(u8, arg, "--kind")) {
                    if (!valid_kind(value)) {
                        std.debug.print("Error: --kind must be technical, product, business, project, or unspecified\n", .{});
                        std.process.exit(1);
                    }
                    filter_kind = value;
                } else if (std.mem.eql(u8, arg, "--limit")) {
                    limit = std.fmt.parseInt(usize, value, 10) catch {
                        std.debug.print("Error: --limit must be a non-negative integer\n", .{});
                        std.process.exit(1);
                    };
                } else if (std.mem.eql(u8, arg, "--group")) {
                    if (!std.mem.eql(u8, value, "kind")) {
                        std.debug.print("Error: --group currently supports only 'kind'\n", .{});
                        std.process.exit(1);
                    }
                    group_kind = true;
                } else format = parse_format_arg(value, false, cmd);
            } else {
                std.debug.print("Error: Unknown flag '{s}'\n", .{arg});
                std.process.exit(1);
            }
        }
        if (filter_project == null) {
            filter_project = resolve_project(arena, io, init.environ_map, null) catch |err| {
                std.debug.print("Error: Failed to resolve project: {any}\n", .{err});
                std.process.exit(1);
            };
        }
        const output_format = format orelse default_format(io, .plain);
        var dir = open_vault_dir(io, vault_path, true) orelse {
            var empty_buf: [4]u8 = undefined;
            var empty_writer = std.Io.File.stdout().writer(io, &empty_buf);
            if (output_format == .json) try empty_writer.interface.writeAll("[]\n");
            try empty_writer.end();
            return;
        };
        defer dir.close(io);
        const ideas = try collect_ideas_filtered(arena, io, dir, filter_project, null, null, filter_kind, archive_filter);
        std.sort.block(IdeaMeta, ideas, {}, IdeaMeta.contextLessThan);
        const selected = ideas[0..@min(limit, ideas.len)];
        var out_buf: [4096]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        switch (output_format) {
            .json => {
                try w.interface.writeAll("[");
                for (selected, 0..) |meta, i| try emit_idea_json(&w.interface, meta, i == 0);
                try w.interface.writeAll("]\n");
            },
            .plain => {
                if (selected.len > 0) try w.interface.print("{s} proposals for {s}:\n", .{ switch (archive_filter) {
                    .active => "Active",
                    .archived => "Archived",
                    .all => "All",
                }, filter_project.? });
                if (group_kind) {
                    for (kind_names) |kind| {
                        var has_kind = false;
                        for (selected) |meta| {
                            if (std.mem.eql(u8, meta.kind, kind)) {
                                if (!has_kind) {
                                    try w.interface.print("\n{s}:\n", .{kind_label(kind)});
                                    has_kind = true;
                                }
                                try w.interface.print("- [{s}] {s}", .{ meta.id, meta.title });
                                if (meta.tags.len > 0) try w.interface.print(" [{s}]", .{meta.tags});
                                if (meta.priority.len > 0) try w.interface.print(" ({s})", .{meta.priority});
                                try w.interface.writeAll("\n");
                            }
                        }
                    }
                } else {
                    for (selected) |meta| {
                        try w.interface.print("- [{s}] [{s}] {s}", .{ meta.id, meta.kind, meta.title });
                        if (meta.tags.len > 0) try w.interface.print(" [{s}]", .{meta.tags});
                        if (meta.priority.len > 0) try w.interface.print(" ({s})", .{meta.priority});
                        try w.interface.writeAll("\n");
                    }
                }
            },
            .table => unreachable,
        }
        try w.end();

        // ── doctor ────────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "doctor")) {
        var repair = false;
        var strict = false;
        var format: ?OutputFormat = null;
        var idx: usize = 2;
        while (idx < args.len) : (idx += 1) {
            const arg = args[idx];
            if (std.mem.eql(u8, arg, "--repair")) {
                repair = true;
            } else if (std.mem.eql(u8, arg, "--strict")) {
                strict = true;
            } else if (std.mem.eql(u8, arg, "--format")) {
                format = parse_format_arg(require_flag_value(args, &idx, arg), false, cmd);
            } else {
                std.debug.print("Error: Unknown flag '{s}'\n", .{arg});
                std.process.exit(2);
            }
        }
        const output_format = format orelse default_format(io, .plain);
        var dir = open_vault_dir(io, vault_path, true) orelse {
            const empty_scan = VaultScan{ .ideas = &.{}, .issues = &.{}, .files_scanned = 0 };
            try emit_doctor_report(io, vault_path, empty_scan, 0, output_format);
            return;
        };
        defer dir.close(io);
        const repaired = if (repair) try repair_vault(arena, io, dir) else 0;
        const scan = try scan_vault(arena, io, dir);
        try emit_doctor_report(io, vault_path, scan, repaired, output_format);
        if (scan_has_failures(scan, strict)) std.process.exit(1);

        // ── archive / unarchive ───────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "archive") or std.mem.eql(u8, cmd, "unarchive")) {
        if (args.len < 3) {
            std.debug.print("Error: '{s}' requires an ID, ID prefix, or filename\n", .{cmd});
            std.process.exit(1);
        }
        const selector = args[2];
        const is_archive = std.mem.eql(u8, cmd, "archive");
        var resolution: ?[]const u8 = null;
        var note: ?[]const u8 = null;
        var format: ?OutputFormat = null;
        var idx: usize = 3;
        while (idx < args.len) : (idx += 1) {
            const arg = args[idx];
            if (std.mem.eql(u8, arg, "--format")) {
                format = parse_format_arg(require_flag_value(args, &idx, arg), false, cmd);
            } else if (is_archive and std.mem.eql(u8, arg, "--resolution")) {
                const value = require_flag_value(args, &idx, arg);
                if (!valid_resolution(value)) {
                    std.debug.print("Error: --resolution must be implemented, rejected, superseded, or stale\n", .{});
                    std.process.exit(1);
                }
                resolution = value;
            } else if (is_archive and std.mem.eql(u8, arg, "--note")) {
                note = require_flag_value(args, &idx, arg);
            } else {
                std.debug.print("Error: Unknown flag '{s}'\n", .{arg});
                std.process.exit(1);
            }
        }
        if (!validate_filename(selector)) {
            std.debug.print("Error: Invalid selector '{s}'\n", .{selector});
            std.process.exit(1);
        }
        var dir = open_vault_or_exit(io, vault_path);
        defer dir.close(io);
        const filename = resolve_selector_or_exit(arena, io, dir, selector);
        const content = try dir.readFileAlloc(io, filename, arena, .unlimited);
        var validation_issues: std.ArrayList(Issue) = .empty;
        const raw_meta = (try parse_front_matter_detailed(arena, content, filename, &validation_issues)) orelse {
            std.debug.print("Error: Cannot {s} malformed pin '{s}'\n", .{ cmd, filename });
            std.process.exit(1);
        };
        for (validation_issues.items) |issue| if (issue.severity == .err) {
            std.debug.print("Error: Cannot {s} '{s}': {s}\n", .{ cmd, filename, issue.message });
            std.process.exit(1);
        };
        if (is_archive and raw_meta.archived_at > 0) {
            std.debug.print("Error: '{s}' is already archived\n", .{filename});
            std.process.exit(1);
        }
        if (!is_archive and raw_meta.archived_at == 0) {
            std.debug.print("Error: '{s}' is not archived\n", .{filename});
            std.process.exit(1);
        }

        const idea_id = if (raw_meta.id.len > 0) raw_meta.id else try derive_id(arena, filename);
        var updates: std.ArrayList(FieldUpdate) = .empty;
        if (!raw_meta.has_schema) try updates.append(arena, .{ .key = "schema", .value = "1" });
        if (!raw_meta.has_id) try updates.append(arena, .{ .key = "id", .value = try yaml_quoted(arena, idea_id) });
        if (is_archive) {
            const archived_value = try std.fmt.allocPrint(arena, "{d}", .{std.Io.Timestamp.now(io, .real).toSeconds()});
            try updates.append(arena, .{ .key = "archived_at", .value = archived_value });
            if (resolution) |value| try updates.append(arena, .{ .key = "resolution", .value = try yaml_quoted(arena, value) });
            if (note) |value| try updates.append(arena, .{ .key = "resolution_note", .value = try yaml_quoted(arena, value) });
        } else {
            try updates.append(arena, .{ .key = "archived_at", .value = null });
            try updates.append(arena, .{ .key = "resolution", .value = null });
            try updates.append(arena, .{ .key = "resolution_note", .value = null });
        }
        const rewritten = try rewrite_front_matter(arena, content, updates.items);
        try atomic_write(arena, io, dir, filename, rewritten);

        var out_buf: [512]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        if ((format orelse default_format(io, .plain)) == .json) {
            try w.interface.print("{{\"{s}\":\"", .{if (is_archive) "archived" else "unarchived"});
            try print_escaped_json(&w.interface, idea_id);
            try w.interface.writeAll("\",\"filename\":\"");
            try print_escaped_json(&w.interface, filename);
            try w.interface.writeAll("\"}\n");
        } else {
            try w.interface.print("{s} {s}  {s}\n", .{ if (is_archive) "Archived" else "Unarchived", idea_id, filename });
        }
        try w.end();

        // ── import / export ───────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "import") or std.mem.eql(u8, cmd, "export")) {
        if (args.len < 3) {
            std.debug.print("Error: '{s}' requires a directory\n", .{cmd});
            std.process.exit(1);
        }
        const path = args[2];
        var force = false;
        var format: ?OutputFormat = null;
        var idx: usize = 3;
        while (idx < args.len) : (idx += 1) {
            if (std.mem.eql(u8, args[idx], "--force")) {
                force = true;
            } else if (std.mem.eql(u8, args[idx], "--format")) {
                format = parse_format_arg(require_flag_value(args, &idx, "--format"), false, cmd);
            } else {
                std.debug.print("Error: Unknown flag '{s}'\n", .{args[idx]});
                std.process.exit(1);
            }
        }
        const is_import = std.mem.eql(u8, cmd, "import");
        if (is_import) try std.Io.Dir.cwd().createDirPath(io, vault_path) else try std.Io.Dir.cwd().createDirPath(io, path);

        var source = if (is_import)
            std.Io.Dir.cwd().openDir(io, path, .{ .iterate = true }) catch |err| {
                std.debug.print("Error: Failed to open import directory '{s}': {any}\n", .{ path, err });
                std.process.exit(1);
            }
        else
            open_vault_dir(io, vault_path, true) orelse {
                std.debug.print("Error: Vault not found.\n", .{});
                std.process.exit(1);
            };
        defer source.close(io);
        var destination = if (is_import)
            try std.Io.Dir.openDirAbsolute(io, vault_path, .{})
        else
            try std.Io.Dir.cwd().openDir(io, path, .{});
        defer destination.close(io);

        // Validate every import candidate before writing anything so malformed
        // input cannot leave the destination vault partially populated.
        if (is_import) {
            var validation_iterator = source.iterate();
            while (try validation_iterator.next(io)) |entry| {
                if (entry.kind != .file or !std.mem.endsWith(u8, entry.name, ".md")) continue;
                const content = try source.readFileAlloc(io, entry.name, arena, .unlimited);
                var import_issues: std.ArrayList(Issue) = .empty;
                const meta = try parse_front_matter_detailed(arena, content, entry.name, &import_issues);
                var invalid = meta == null;
                for (import_issues.items) |issue| if (issue.severity == .err) {
                    invalid = true;
                    break;
                };
                if (invalid) {
                    std.debug.print("Error: '{s}' is not a valid pin file\n", .{entry.name});
                    std.process.exit(1);
                }
            }
        }

        var copied: usize = 0;
        var skipped: usize = 0;
        var iterator = source.iterate();
        while (try iterator.next(io)) |entry| {
            if (entry.kind != .file or !std.mem.endsWith(u8, entry.name, ".md")) continue;
            if (!force) {
                if (destination.statFile(io, entry.name, .{})) |_| {
                    skipped += 1;
                    continue;
                } else |err| switch (err) {
                    error.FileNotFound => {},
                    else => return err,
                }
            }
            const content = try source.readFileAlloc(io, entry.name, arena, .unlimited);
            try destination.writeFile(io, .{ .sub_path = entry.name, .data = content });
            copied += 1;
        }

        var out_buf: [256]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        if ((format orelse default_format(io, .plain)) == .json) {
            try w.interface.print("{{\"operation\":\"{s}\",\"copied\":{d},\"skipped\":{d}}}\n", .{ cmd, copied, skipped });
        } else try w.interface.print("{s}: {d} copied, {d} skipped\n", .{ cmd, copied, skipped });
        try w.end();

        // ── read ─────────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "read")) {
        const selector_args = parse_selector_args(args, cmd);
        const format = selector_args.format orelse .plain;
        var dir = open_vault_or_exit(io, vault_path);
        defer dir.close(io);
        const filename = resolve_selector_or_exit(arena, io, dir, selector_args.selector);

        const content = dir.readFileAlloc(io, filename, arena, .unlimited) catch |err| {
            if (err == error.FileNotFound) {
                std.debug.print("Error: File '{s}' not found in vault.\n", .{filename});
            } else {
                std.debug.print("Error: Failed to read file '{s}': {any}\n", .{ filename, err });
            }
            std.process.exit(1);
        };

        if (format == .plain) {
            const stdout_file = std.Io.File.stdout();
            stdout_file.writeStreamingAll(io, content) catch |err| {
                std.debug.print("Error: Failed to write to stdout: {any}\n", .{err});
                std.process.exit(1);
            };
        } else {
            var out_buf: [4096]u8 = undefined;
            var w = std.Io.File.stdout().writer(io, &out_buf);
            try w.interface.writeAll("{\"filename\":\"");
            try print_escaped_json(&w.interface, filename);
            try w.interface.writeAll("\",\"content\":\"");
            try print_escaped_json(&w.interface, content);
            try w.interface.writeAll("\"}\n");
            try w.end();
        }

        // ── rm ───────────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "rm")) {
        const selector_args = parse_selector_args(args, cmd);
        var dir = open_vault_or_exit(io, vault_path);
        defer dir.close(io);
        const filename = resolve_selector_or_exit(arena, io, dir, selector_args.selector);
        const removed_id = try idea_id_for_file(arena, io, dir, filename);
        dir.deleteFile(io, filename) catch |err| {
            std.debug.print("Error: Failed to delete file '{s}': {any}\n", .{ filename, err });
            std.process.exit(1);
        };

        var out_buf: [256]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        if ((selector_args.format orelse default_format(io, .plain)) == .json) {
            try w.interface.writeAll("{\"removed\":\"");
            try print_escaped_json(&w.interface, removed_id);
            try w.interface.writeAll("\",\"filename\":\"");
            try print_escaped_json(&w.interface, filename);
            try w.interface.writeAll("\"}\n");
        } else try w.interface.print("Removed {s}  {s}\n", .{ removed_id, filename });
        try w.end();

        // ── edit ─────────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "edit")) {
        const selector_args = parse_selector_args(args, cmd);
        var dir = open_vault_or_exit(io, vault_path);
        const filename = resolve_selector_or_exit(arena, io, dir, selector_args.selector);
        const original_content = try dir.readFileAlloc(io, filename, arena, .unlimited);
        const edited_id = try idea_id_for_file(arena, io, dir, filename);
        dir.close(io);

        const full_path = try std.fs.path.join(arena, &.{ vault_path, filename });

        const fallback_editor: []const u8 = if (builtin.os.tag == .windows) "notepad" else "vi";
        const editor = init.environ_map.get("EDITOR") orelse
            init.environ_map.get("VISUAL") orelse
            fallback_editor;

        var editor_args: std.ArrayList([]const u8) = .empty;
        var words = std.mem.tokenizeAny(u8, editor, " \t");
        while (words.next()) |word| try editor_args.append(arena, word);
        if (editor_args.items.len == 0) try editor_args.append(arena, fallback_editor);
        try editor_args.append(arena, full_path);

        var child = std.process.spawn(io, .{
            .argv = editor_args.items,
            .stdin = .inherit,
            .stdout = .inherit,
            .stderr = .inherit,
        }) catch |err| {
            std.debug.print("Error: Failed to launch editor '{s}': {any}\n", .{ editor, err });
            std.process.exit(1);
        };
        const term = try child.wait(io);

        switch (term) {
            .exited => |code| {
                if (code != 0) {
                    std.debug.print("Editor exited with code {d}\n", .{code});
                    std.process.exit(1);
                }
            },
            else => {
                std.debug.print("Editor terminated abnormally\n", .{});
                std.process.exit(1);
            },
        }

        var validation_dir = open_vault_or_exit(io, vault_path);
        defer validation_dir.close(io);
        const edited_content = validation_dir.readFileAlloc(io, filename, arena, .unlimited) catch |err| {
            try atomic_write(arena, io, validation_dir, filename, original_content);
            std.debug.print("Error: Editor removed or made '{s}' unreadable ({any}); original restored\n", .{ filename, err });
            std.process.exit(1);
        };
        var edit_issues: std.ArrayList(Issue) = .empty;
        const edited_meta = try parse_front_matter_detailed(arena, edited_content, filename, &edit_issues);
        var invalid_edit = edited_meta == null;
        for (edit_issues.items) |issue| if (issue.severity == .err) {
            invalid_edit = true;
            std.debug.print("Error: {s}: {s} [{s}]\n", .{ filename, issue.message, issue.code });
        };
        if (invalid_edit) {
            const recovery_name = try std.fmt.allocPrint(arena, ".{s}.edit-recovery.tmp", .{edited_id});
            try validation_dir.writeFile(io, .{ .sub_path = recovery_name, .data = edited_content });
            try atomic_write(arena, io, validation_dir, filename, original_content);
            std.debug.print("Error: Invalid edit was not applied. Recovery saved to {s}/{s}\n", .{ vault_path, recovery_name });
            std.process.exit(1);
        }

        var out_buf: [256]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        if ((selector_args.format orelse default_format(io, .plain)) == .json) {
            try w.interface.writeAll("{\"edited\":\"");
            try print_escaped_json(&w.interface, edited_id);
            try w.interface.writeAll("\",\"filename\":\"");
            try print_escaped_json(&w.interface, filename);
            try w.interface.writeAll("\"}\n");
        } else try w.interface.print("Edited {s}  {s}\n", .{ edited_id, filename });
        try w.end();

        // ── stats ────────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "stats")) {
        var format: ?OutputFormat = null;
        var idx: usize = 2;
        while (idx < args.len) : (idx += 1) {
            if (std.mem.eql(u8, args[idx], "--format")) {
                format = parse_format_arg(require_flag_value(args, &idx, "--format"), false, cmd);
            } else {
                std.debug.print("Error: Unexpected argument '{s}'\n", .{args[idx]});
                std.process.exit(1);
            }
        }
        const output_format = format orelse default_format(io, .plain);
        var dir = open_vault_dir(io, vault_path, true) orelse {
            var out_buf: [std.fs.max_path_bytes + 128]u8 = undefined;
            var w = std.Io.File.stdout().writer(io, &out_buf);
            if (output_format == .json) {
                try w.interface.writeAll("{\"vault\":\"");
                try print_escaped_json(&w.interface, vault_path);
                try w.interface.writeAll("\",\"ideas\":0,\"active\":0,\"archived\":0,\"invalid\":0,\"projects\":[],\"tags\":[],\"kinds\":{\"technical\":0,\"product\":0,\"business\":0,\"project\":0,\"unspecified\":0}}\n");
            } else try w.interface.print("Vault:    {s}\nIdeas:    0\nActive:   0\nArchived: 0\nInvalid:  0\n", .{vault_path});
            try w.end();
            return;
        };
        defer dir.close(io);

        const ideas = try collect_ideas_filtered(arena, io, dir, null, null, null, null, .all);
        const scan = try scan_vault(arena, io, dir);
        const invalid_count = scan.files_scanned - ideas.len;

        var total: usize = 0;
        var active_count: usize = 0;
        var archived_count: usize = 0;
        var oldest: i64 = std.math.maxInt(i64);
        var newest: i64 = 0;
        var kind_counts: [5]usize = .{0} ** 5;

        // Count unique projects, tags, and domains
        var projects: std.ArrayList([]const u8) = .empty;
        var tag_list: std.ArrayList([]const u8) = .empty;

        for (ideas) |meta| {
            total += 1;
            if (meta.archived_at > 0) archived_count += 1 else active_count += 1;
            kind_counts[kind_index(meta.kind)] += 1;
            if (meta.timestamp < oldest) oldest = meta.timestamp;
            if (meta.timestamp > newest) newest = meta.timestamp;

            // Deduplicate projects
            var found = false;
            for (projects.items) |p| {
                if (std.mem.eql(u8, p, meta.project)) {
                    found = true;
                    break;
                }
            }
            if (!found and meta.project.len > 0) {
                try projects.append(arena, meta.project);
            }

            // Deduplicate tags
            if (meta.tags.len > 0) {
                var pos: usize = 0;
                while (pos < meta.tags.len) {
                    const end = std.mem.indexOfScalarPos(u8, meta.tags, pos, ',') orelse meta.tags.len;
                    const tag = std.mem.trim(u8, meta.tags[pos..end], " ");
                    if (tag.len > 0) {
                        var tag_found = false;
                        for (tag_list.items) |t| {
                            if (std.ascii.eqlIgnoreCase(t, tag)) {
                                tag_found = true;
                                break;
                            }
                        }
                        if (!tag_found) try tag_list.append(arena, tag);
                    }
                    if (end >= meta.tags.len) break;
                    pos = end + 1;
                }
            }
        }

        var out_buf: [4096]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);

        if (output_format == .json) {
            try w.interface.writeAll("{\"vault\":\"");
            try print_escaped_json(&w.interface, vault_path);
            try w.interface.print("\",\"ideas\":{d},\"active\":{d},\"archived\":{d},\"invalid\":{d},\"projects\":[", .{ total, active_count, archived_count, invalid_count });
            for (projects.items, 0..) |project, i| {
                if (i > 0) try w.interface.writeAll(",");
                try w.interface.writeAll("\"");
                try print_escaped_json(&w.interface, project);
                try w.interface.writeAll("\"");
            }
            try w.interface.writeAll("],\"tags\":[");
            for (tag_list.items, 0..) |tag, i| {
                if (i > 0) try w.interface.writeAll(",");
                try w.interface.writeAll("\"");
                try print_escaped_json(&w.interface, tag);
                try w.interface.writeAll("\"");
            }
            try w.interface.writeAll("],\"kinds\":{");
            for (kind_names, 0..) |kind, i| {
                if (i > 0) try w.interface.writeAll(",");
                try w.interface.print("\"{s}\":{d}", .{ kind, kind_counts[i] });
            }
            try w.interface.writeAll("}");
            if (total > 0) try w.interface.print(",\"oldest\":{d},\"newest\":{d}", .{ oldest, newest });
            try w.interface.writeAll("}\n");
        } else {
            try w.interface.print("Vault:    {s}\n", .{vault_path});
            try w.interface.print("Ideas:    {d}\n", .{total});
            try w.interface.print("Active:   {d}\nArchived: {d}\nInvalid:  {d}\n", .{ active_count, archived_count, invalid_count });
            try w.interface.print("Projects: {d}\n", .{projects.items.len});
            if (projects.items.len > 0) {
                try w.interface.writeAll("          ");
                for (projects.items, 0..) |project, i| {
                    if (i > 0) try w.interface.writeAll(", ");
                    try w.interface.print("{s}", .{project});
                }
                try w.interface.writeAll("\n");
            }
            try w.interface.writeAll("Kinds:    ");
            for (kind_names, 0..) |kind, i| {
                if (i > 0) try w.interface.writeAll(", ");
                try w.interface.print("{s}={d}", .{ kind, kind_counts[i] });
            }
            try w.interface.writeAll("\n");
            if (tag_list.items.len > 0) {
                try w.interface.print("Tags:     {d}\n          ", .{tag_list.items.len});
                for (tag_list.items, 0..) |tag, i| {
                    if (i > 0) try w.interface.writeAll(", ");
                    try w.interface.print("{s}", .{tag});
                }
                try w.interface.writeAll("\n");
            }
            if (total > 0) {
                try w.interface.writeAll("Oldest:   ");
                try format_date(&w.interface, oldest);
                try w.interface.writeAll("\nNewest:   ");
                try format_date(&w.interface, newest);
                try w.interface.writeAll("\n");
            }
        }

        try w.end();
    } else {
        std.debug.print("Error: Unknown subcommand '{s}'\n\n", .{cmd});
        try print_usage(io);
        std.process.exit(1);
    }
}

test "front matter diagnostics reject malformed values" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();
    const content =
        "---\n" ++
        "schema: 1\n" ++
        "id: \"abcdef123456\"\n" ++
        "project: \"example\"\n" ++
        "kind: \"unknown\"\n" ++
        "timestamp: nope\n" ++
        "title: \"Broken\"\n" ++
        "---\n" ++
        "# Broken\n";
    var issues: std.ArrayList(Issue) = .empty;
    _ = try parse_front_matter_detailed(arena, content, "broken.md", &issues);
    var saw_kind = false;
    var saw_integer = false;
    for (issues.items) |issue| {
        if (std.mem.eql(u8, issue.code, "invalid_kind")) saw_kind = true;
        if (std.mem.eql(u8, issue.code, "invalid_integer")) saw_integer = true;
    }
    try std.testing.expect(saw_kind);
    try std.testing.expect(saw_integer);
    try std.testing.expect((try parse_front_matter_from_buf(arena, content)) == null);
}

test "front matter rewrite preserves body and unknown fields" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();
    const content =
        "---\n" ++
        "project: \"example\"\n" ++
        "custom: \"keep me\"\n" ++
        "title: \"Legacy\"\n" ++
        "---\n" ++
        "# Legacy\n\nBody.\n";
    const rewritten = try rewrite_front_matter(arena, content, &.{
        .{ .key = "schema", .value = "1" },
        .{ .key = "id", .value = "\"abcdef123456\"" },
    });
    try std.testing.expect(std.mem.indexOf(u8, rewritten, "custom: \"keep me\"") != null);
    try std.testing.expect(std.mem.endsWith(u8, rewritten, "# Legacy\n\nBody.\n"));
}

test "search ranks title matches above body matches" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();
    const title_content =
        "---\nproject: \"p\"\nkind: \"product\"\ntimestamp: 1\ntitle: \"Ranked results\"\n---\nBody.\n";
    const body_content =
        "---\nproject: \"p\"\nkind: \"technical\"\ntimestamp: 2\ntitle: \"Incidental\"\n---\nRanked results appear here.\n";
    const title_meta = (try parse_front_matter_from_buf(arena, title_content)).?;
    const body_meta = (try parse_front_matter_from_buf(arena, body_content)).?;
    try std.testing.expect(search_score(title_meta, "ranked results").? > search_score(body_meta, "ranked results").?);
}
