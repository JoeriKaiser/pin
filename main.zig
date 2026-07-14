const std = @import("std");

const version = "0.3.0";

const OutputFormat = enum { json, table, plain };

const IdeaMeta = struct {
    id: []const u8,
    filename: []const u8,
    project: []const u8,
    kind: []const u8,
    timestamp: i64,
    created_at_ns: i128,
    title: []const u8,
    tags: []const u8,
    priority: []const u8,

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

/// Parse YAML front matter from an in-memory buffer.
fn parse_front_matter_from_buf(arena: std.mem.Allocator, content: []const u8) !?IdeaMeta {
    if (content.len < 4) return null;

    // First line must be "---"
    const first_nl = std.mem.indexOfScalar(u8, content, '\n') orelse return null;
    const first_line = std.mem.trim(u8, content[0..first_nl], " \r");
    if (!std.mem.eql(u8, first_line, "---")) return null;

    var id: ?[]const u8 = null;
    var project: ?[]const u8 = null;
    var kind: ?[]const u8 = null;
    var timestamp: ?i64 = null;
    var created_at_ns: ?i128 = null;
    var title: ?[]const u8 = null;
    var tags: ?[]const u8 = null;
    var priority: ?[]const u8 = null;

    var pos = first_nl + 1;
    while (pos < content.len) {
        const line_end = std.mem.indexOfScalarPos(u8, content, pos, '\n') orelse content.len;
        const line = std.mem.trim(u8, content[pos..line_end], " \r");
        pos = line_end + 1;

        if (std.mem.eql(u8, line, "---")) break;

        const colon_idx = std.mem.indexOfScalar(u8, line, ':') orelse continue;
        const key = std.mem.trim(u8, line[0..colon_idx], " ");
        const value = std.mem.trim(u8, line[colon_idx + 1 ..], " ");

        if (std.mem.eql(u8, key, "id")) {
            id = try unescape_yaml_string(arena, value);
        } else if (std.mem.eql(u8, key, "project")) {
            project = try unescape_yaml_string(arena, value);
        } else if (std.mem.eql(u8, key, "kind")) {
            kind = try unescape_yaml_string(arena, value);
        } else if (std.mem.eql(u8, key, "title")) {
            title = try unescape_yaml_string(arena, value);
        } else if (std.mem.eql(u8, key, "timestamp")) {
            timestamp = std.fmt.parseInt(i64, value, 10) catch 0;
        } else if (std.mem.eql(u8, key, "created_at_ns")) {
            created_at_ns = std.fmt.parseInt(i128, value, 10) catch 0;
        } else if (std.mem.eql(u8, key, "tags")) {
            tags = try unescape_yaml_string(arena, value);
        } else if (std.mem.eql(u8, key, "priority")) {
            priority = try unescape_yaml_string(arena, value);
        }
    }

    return IdeaMeta{
        .id = id orelse "",
        .filename = "",
        .project = project orelse "",
        .kind = if (kind) |value| if (valid_kind(value)) value else "unspecified" else "unspecified",
        .timestamp = timestamp orelse 0,
        .created_at_ns = created_at_ns orelse 0,
        .title = title orelse "",
        .tags = tags orelse "",
        .priority = priority orelse "",
    };
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

fn collect_ideas(
    arena: std.mem.Allocator,
    io: std.Io,
    dir: std.Io.Dir,
    filter_project: ?[]const u8,
    filter_query: ?[]const u8,
    filter_tag: ?[]const u8,
    filter_kind: ?[]const u8,
) ![]IdeaMeta {
    var list: std.ArrayList(IdeaMeta) = .empty;
    var it = dir.iterate();

    while (try it.next(io)) |entry| {
        if (entry.kind != .file or !std.mem.endsWith(u8, entry.name, ".md")) continue;

        const content = dir.readFileAlloc(io, entry.name, arena, .unlimited) catch continue;

        // Query filter: search full file content
        if (filter_query) |query| {
            if (std.ascii.findIgnoreCase(content, query) == null) continue;
        }

        const meta_opt = try parse_front_matter_from_buf(arena, content);
        if (meta_opt) |raw_meta| {
            // Project filter
            if (filter_project) |proj| {
                if (!std.mem.eql(u8, raw_meta.project, proj)) continue;
            }

            // Tag and domain filters
            if (filter_tag) |tag| {
                if (!tag_matches(raw_meta.tags, tag)) continue;
            }
            if (filter_kind) |kind| {
                if (!std.ascii.eqlIgnoreCase(raw_meta.kind, kind)) continue;
            }

            try list.append(arena, .{
                .id = if (raw_meta.id.len > 0) raw_meta.id else try derive_id(arena, entry.name),
                .filename = try arena.dupe(u8, entry.name),
                .project = raw_meta.project,
                .kind = raw_meta.kind,
                .timestamp = raw_meta.timestamp,
                .created_at_ns = raw_meta.created_at_ns,
                .title = raw_meta.title,
                .tags = raw_meta.tags,
                .priority = raw_meta.priority,
            });
        }
    }

    const items = list.items;
    std.sort.block(IdeaMeta, items, {}, IdeaMeta.lessThan);
    return items;
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

    const ideas = try collect_ideas(arena, io, dir, null, null, null, null);
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
        \\  pin list [--project <name>] [--tag <name>] [--kind <kind>] [--format json|table|plain]
        \\  pin list-project [--tag <name>] [--kind <kind>] [--format json|table|plain]
        \\  pin search "<query>" [--project <name>] [--tag <name>] [--kind <kind>] [--format json|table|plain]
        \\  pin context [--project <name>] [--kind <kind>] [--limit <n>] [--group kind] [--format json|plain]
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
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: {s} requires a value\n", .{flag});
                    std.process.exit(1);
                }
                idx += 1;
                if (std.mem.eql(u8, flag, "--project")) project = args[idx] else {
                    format = parse_format(args[idx]) orelse {
                        std.debug.print("Error: --format must be json or plain for 'init'\n", .{});
                        std.process.exit(1);
                    };
                    if (format.? == .table) {
                        std.debug.print("Error: --format must be json or plain for 'init'\n", .{});
                        std.process.exit(1);
                    }
                }
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
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: {s} requires a value\n", .{arg});
                    std.process.exit(1);
                }
                idx += 1;
                if (std.mem.eql(u8, arg, "--project")) project = args[idx] else if (std.mem.eql(u8, arg, "--title")) title = args[idx] else if (std.mem.eql(u8, arg, "--tags")) tags = args[idx] else if (std.mem.eql(u8, arg, "--kind")) {
                    if (!valid_kind(args[idx]) or std.mem.eql(u8, args[idx], "unspecified")) {
                        std.debug.print("Error: --kind must be technical, product, business, or project\n", .{});
                        std.process.exit(1);
                    }
                    kind = args[idx];
                } else if (std.mem.eql(u8, arg, "--priority")) {
                    if (!valid_priority(args[idx])) {
                        std.debug.print("Error: --priority must be low, medium, or high\n", .{});
                        std.process.exit(1);
                    }
                    priority = args[idx];
                } else {
                    format = parse_format(args[idx]) orelse {
                        std.debug.print("Error: --format must be json or plain for 'add'\n", .{});
                        std.process.exit(1);
                    };
                    if (format.? == .table) {
                        std.debug.print("Error: --format must be json or plain for 'add'\n", .{});
                        std.process.exit(1);
                    }
                }
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

        const meta = IdeaMeta{ .id = id, .filename = filename, .project = proj_name, .kind = kind_val, .timestamp = seconds, .created_at_ns = total_ns, .title = title_val, .tags = tags orelse "", .priority = priority orelse "" };
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
            if (std.mem.eql(u8, arg, "--project") or std.mem.eql(u8, arg, "--tag") or std.mem.eql(u8, arg, "--kind") or std.mem.eql(u8, arg, "--format")) {
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: {s} requires a value\n", .{arg});
                    std.process.exit(1);
                }
                idx += 1;
                if (std.mem.eql(u8, arg, "--project")) filter_project = args[idx] else if (std.mem.eql(u8, arg, "--tag")) filter_tag = args[idx] else if (std.mem.eql(u8, arg, "--kind")) {
                    if (!valid_kind(args[idx])) {
                        std.debug.print("Error: --kind must be technical, product, business, project, or unspecified\n", .{});
                        std.process.exit(1);
                    }
                    filter_kind = args[idx];
                } else format = parse_format(args[idx]) orelse {
                    std.debug.print("Error: --format must be json, table, or plain\n", .{});
                    std.process.exit(1);
                };
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

        const ideas = try collect_ideas(arena, io, dir, filter_project, null, filter_tag, filter_kind);

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
        var format: ?OutputFormat = null;

        var idx: usize = 3;
        while (idx < args.len) : (idx += 1) {
            const arg = args[idx];
            if (std.mem.eql(u8, arg, "--project") or std.mem.eql(u8, arg, "--tag") or std.mem.eql(u8, arg, "--kind") or std.mem.eql(u8, arg, "--format")) {
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: {s} requires a value\n", .{arg});
                    std.process.exit(1);
                }
                idx += 1;
                if (std.mem.eql(u8, arg, "--project")) filter_project = args[idx] else if (std.mem.eql(u8, arg, "--tag")) filter_tag = args[idx] else if (std.mem.eql(u8, arg, "--kind")) {
                    if (!valid_kind(args[idx])) {
                        std.debug.print("Error: --kind must be technical, product, business, project, or unspecified\n", .{});
                        std.process.exit(1);
                    }
                    filter_kind = args[idx];
                } else format = parse_format(args[idx]) orelse {
                    std.debug.print("Error: --format must be json, table, or plain\n", .{});
                    std.process.exit(1);
                };
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

        const ideas = try collect_ideas(arena, io, dir, filter_project, query, filter_tag, filter_kind);

        var out_buf: [4096]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);

        switch (output_format) {
            .table => if (ideas.len == 0) {
                try w.interface.print("No results for \"{s}\".\n", .{query});
            } else {
                try w.interface.writeAll("DATE        PROJECT           KIND         ID            TITLE\n");
                try w.interface.writeAll("----------  ----------------  -----------  ------------  ----------------------------------------\n");
                for (ideas) |meta| try emit_idea_table(&w.interface, meta);
                try w.interface.print("\n{d} result(s) for \"{s}\"\n", .{ ideas.len, query });
            },
            .plain => for (ideas) |meta| try emit_idea_plain(&w.interface, meta),
            .json => {
                try w.interface.writeAll("[");
                for (ideas, 0..) |meta, i| try emit_idea_json(&w.interface, meta, i == 0);
                try w.interface.writeAll("]\n");
            },
        }
        try w.end();

        // ── context ──────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "context")) {
        var filter_project: ?[]const u8 = null;
        var filter_kind: ?[]const u8 = null;
        var limit: usize = 10;
        var group_kind = false;
        var format: ?OutputFormat = null;
        var idx: usize = 2;
        while (idx < args.len) : (idx += 1) {
            const arg = args[idx];
            if (std.mem.eql(u8, arg, "--project") or std.mem.eql(u8, arg, "--kind") or std.mem.eql(u8, arg, "--limit") or std.mem.eql(u8, arg, "--group") or std.mem.eql(u8, arg, "--format")) {
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: {s} requires a value\n", .{arg});
                    std.process.exit(1);
                }
                idx += 1;
                if (std.mem.eql(u8, arg, "--project")) filter_project = args[idx] else if (std.mem.eql(u8, arg, "--kind")) {
                    if (!valid_kind(args[idx])) {
                        std.debug.print("Error: --kind must be technical, product, business, project, or unspecified\n", .{});
                        std.process.exit(1);
                    }
                    filter_kind = args[idx];
                } else if (std.mem.eql(u8, arg, "--limit")) {
                    limit = std.fmt.parseInt(usize, args[idx], 10) catch {
                        std.debug.print("Error: --limit must be a non-negative integer\n", .{});
                        std.process.exit(1);
                    };
                } else if (std.mem.eql(u8, arg, "--group")) {
                    if (!std.mem.eql(u8, args[idx], "kind")) {
                        std.debug.print("Error: --group currently supports only 'kind'\n", .{});
                        std.process.exit(1);
                    }
                    group_kind = true;
                } else {
                    format = parse_format(args[idx]) orelse {
                        std.debug.print("Error: --format must be json or plain for 'context'\n", .{});
                        std.process.exit(1);
                    };
                    if (format.? == .table) {
                        std.debug.print("Error: --format must be json or plain for 'context'\n", .{});
                        std.process.exit(1);
                    }
                }
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
        const ideas = try collect_ideas(arena, io, dir, filter_project, null, null, filter_kind);
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
                if (selected.len > 0) try w.interface.print("Active proposals for {s}:\n", .{filter_project.?});
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
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: --format requires a value\n", .{});
                    std.process.exit(1);
                }
                idx += 1;
                format = parse_format(args[idx]) orelse {
                    std.debug.print("Error: --format must be json or plain for '{s}'\n", .{cmd});
                    std.process.exit(1);
                };
                if (format.? == .table) {
                    std.debug.print("Error: --format must be json or plain for '{s}'\n", .{cmd});
                    std.process.exit(1);
                }
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
            if (is_import) {
                const meta = try parse_front_matter_from_buf(arena, content);
                if (meta == null or meta.?.project.len == 0 or meta.?.title.len == 0) {
                    std.debug.print("Error: '{s}' is not a valid pin file\n", .{entry.name});
                    std.process.exit(1);
                }
            }
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
        if (args.len != 3 and args.len != 5) {
            std.debug.print("Error: 'read' requires an ID, ID prefix, or filename and optional --format\n", .{});
            std.process.exit(1);
        }
        const selector = args[2];
        var format: OutputFormat = .plain;
        if (args.len == 5) {
            if (!std.mem.eql(u8, args[3], "--format")) {
                std.debug.print("Error: Unexpected argument '{s}'\n", .{args[3]});
                std.process.exit(1);
            }
            format = parse_format(args[4]) orelse {
                std.debug.print("Error: --format must be json or plain for 'read'\n", .{});
                std.process.exit(1);
            };
            if (format == .table) {
                std.debug.print("Error: --format must be json or plain for 'read'\n", .{});
                std.process.exit(1);
            }
        }
        if (!validate_filename(selector)) {
            std.debug.print("Error: Invalid selector '{s}'\n", .{selector});
            std.process.exit(1);
        }

        var dir = std.Io.Dir.openDirAbsolute(io, vault_path, .{ .iterate = true }) catch |err| {
            if (err == error.FileNotFound) std.debug.print("Error: Vault not found.\n", .{}) else std.debug.print("Error: Failed to open vault directory: {any}\n", .{err});
            std.process.exit(1);
        };
        defer dir.close(io);
        const filename = resolve_selector(arena, io, dir, selector) catch |err| {
            if (err == SelectorError.AmbiguousSelector) std.debug.print("Error: Selector '{s}' is ambiguous. Use more ID characters.\n", .{selector}) else if (err == SelectorError.SelectorNotFound) std.debug.print("Error: No idea matches '{s}'.\n", .{selector}) else std.debug.print("Error: Failed to resolve '{s}': {any}\n", .{ selector, err });
            std.process.exit(1);
        };

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
        if (args.len < 3) {
            std.debug.print("Error: 'rm' requires an ID, ID prefix, or filename\n", .{});
            std.process.exit(1);
        }
        const selector = args[2];
        var format: ?OutputFormat = null;
        var idx: usize = 3;
        while (idx < args.len) : (idx += 1) {
            if (!std.mem.eql(u8, args[idx], "--format") or idx + 1 >= args.len) {
                std.debug.print("Error: Unexpected argument '{s}'\n", .{args[idx]});
                std.process.exit(1);
            }
            idx += 1;
            format = parse_format(args[idx]) orelse {
                std.debug.print("Error: --format must be json or plain for 'rm'\n", .{});
                std.process.exit(1);
            };
            if (format.? == .table) {
                std.debug.print("Error: --format must be json or plain for 'rm'\n", .{});
                std.process.exit(1);
            }
        }
        if (!validate_filename(selector)) {
            std.debug.print("Error: Invalid selector '{s}'\n", .{selector});
            std.process.exit(1);
        }

        var dir = std.Io.Dir.openDirAbsolute(io, vault_path, .{ .iterate = true }) catch |err| {
            if (err == error.FileNotFound) std.debug.print("Error: Vault not found.\n", .{}) else std.debug.print("Error: Failed to open vault directory: {any}\n", .{err});
            std.process.exit(1);
        };
        defer dir.close(io);
        const filename = resolve_selector(arena, io, dir, selector) catch |err| {
            if (err == SelectorError.AmbiguousSelector) std.debug.print("Error: Selector '{s}' is ambiguous. Use more ID characters.\n", .{selector}) else if (err == SelectorError.SelectorNotFound) std.debug.print("Error: No idea matches '{s}'.\n", .{selector}) else std.debug.print("Error: Failed to resolve '{s}': {any}\n", .{ selector, err });
            std.process.exit(1);
        };
        const removed_id = blk: {
            const content = try dir.readFileAlloc(io, filename, arena, .unlimited);
            const meta = try parse_front_matter_from_buf(arena, content);
            if (meta) |value| if (value.id.len > 0) break :blk value.id;
            break :blk try derive_id(arena, filename);
        };
        dir.deleteFile(io, filename) catch |err| {
            std.debug.print("Error: Failed to delete file '{s}': {any}\n", .{ filename, err });
            std.process.exit(1);
        };

        var out_buf: [256]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        if ((format orelse default_format(io, .plain)) == .json) {
            try w.interface.writeAll("{\"removed\":\"");
            try print_escaped_json(&w.interface, removed_id);
            try w.interface.writeAll("\",\"filename\":\"");
            try print_escaped_json(&w.interface, filename);
            try w.interface.writeAll("\"}\n");
        } else try w.interface.print("Removed {s}  {s}\n", .{ removed_id, filename });
        try w.end();

        // ── edit ─────────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "edit")) {
        if (args.len != 3 and args.len != 5) {
            std.debug.print("Error: 'edit' requires an ID, ID prefix, or filename and optional --format\n", .{});
            std.process.exit(1);
        }
        const selector = args[2];
        var format: ?OutputFormat = null;
        if (args.len == 5) {
            if (!std.mem.eql(u8, args[3], "--format")) {
                std.debug.print("Error: Unexpected argument '{s}'\n", .{args[3]});
                std.process.exit(1);
            }
            format = parse_format(args[4]) orelse {
                std.debug.print("Error: --format must be json or plain for 'edit'\n", .{});
                std.process.exit(1);
            };
            if (format.? == .table) {
                std.debug.print("Error: --format must be json or plain for 'edit'\n", .{});
                std.process.exit(1);
            }
        }
        if (!validate_filename(selector)) {
            std.debug.print("Error: Invalid selector '{s}'\n", .{selector});
            std.process.exit(1);
        }

        var dir = std.Io.Dir.openDirAbsolute(io, vault_path, .{ .iterate = true }) catch |err| {
            if (err == error.FileNotFound) std.debug.print("Error: Vault not found.\n", .{}) else std.debug.print("Error: Failed to open vault directory: {any}\n", .{err});
            std.process.exit(1);
        };
        const filename = resolve_selector(arena, io, dir, selector) catch |err| {
            if (err == SelectorError.AmbiguousSelector) std.debug.print("Error: Selector '{s}' is ambiguous. Use more ID characters.\n", .{selector}) else if (err == SelectorError.SelectorNotFound) std.debug.print("Error: No idea matches '{s}'.\n", .{selector}) else std.debug.print("Error: Failed to resolve '{s}': {any}\n", .{ selector, err });
            std.process.exit(1);
        };
        const edited_id = blk: {
            const content = try dir.readFileAlloc(io, filename, arena, .unlimited);
            const meta = try parse_front_matter_from_buf(arena, content);
            if (meta) |value| {
                if (value.id.len > 0) break :blk value.id;
            }
            break :blk try derive_id(arena, filename);
        };
        dir.close(io);

        const full_path = try std.fs.path.join(arena, &.{ vault_path, filename });

        const editor = init.environ_map.get("EDITOR") orelse
            init.environ_map.get("VISUAL") orelse
            "vi";

        var editor_args: std.ArrayList([]const u8) = .empty;
        var words = std.mem.tokenizeAny(u8, editor, " \t");
        while (words.next()) |word| try editor_args.append(arena, word);
        if (editor_args.items.len == 0) try editor_args.append(arena, "vi");
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

        var out_buf: [256]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        if ((format orelse default_format(io, .plain)) == .json) {
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
        if (args.len != 2 and args.len != 4) {
            std.debug.print("Error: 'stats' accepts only optional --format json|plain\n", .{});
            std.process.exit(1);
        }
        if (args.len == 4) {
            if (!std.mem.eql(u8, args[2], "--format")) {
                std.debug.print("Error: Unexpected argument '{s}'\n", .{args[2]});
                std.process.exit(1);
            }
            format = parse_format(args[3]) orelse {
                std.debug.print("Error: --format must be json or plain for 'stats'\n", .{});
                std.process.exit(1);
            };
            if (format.? == .table) {
                std.debug.print("Error: --format must be json or plain for 'stats'\n", .{});
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
                try w.interface.writeAll("\",\"ideas\":0,\"projects\":[],\"tags\":[],\"kinds\":{\"technical\":0,\"product\":0,\"business\":0,\"project\":0,\"unspecified\":0}}\n");
            } else try w.interface.print("Vault:    {s}\nIdeas:    0\n", .{vault_path});
            try w.end();
            return;
        };
        defer dir.close(io);

        const ideas = try collect_ideas(arena, io, dir, null, null, null, null);

        var total: usize = 0;
        var oldest: i64 = std.math.maxInt(i64);
        var newest: i64 = 0;
        var kind_counts: [5]usize = .{0} ** 5;

        // Count unique projects, tags, and domains
        var projects: std.ArrayList([]const u8) = .empty;
        var tag_list: std.ArrayList([]const u8) = .empty;

        for (ideas) |meta| {
            total += 1;
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
            try w.interface.print("\",\"ideas\":{d},\"projects\":[", .{total});
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
