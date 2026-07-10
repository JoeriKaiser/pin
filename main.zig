const std = @import("std");

const IdeaMeta = struct {
    filename: []const u8,
    project: []const u8,
    timestamp: i64,
    title: []const u8,
    tags: []const u8,

    fn lessThan(_: void, a: IdeaMeta, b: IdeaMeta) bool {
        return a.timestamp > b.timestamp; // newest first
    }
};

fn get_default_title(allocator: std.mem.Allocator, text: []const u8) ![]const u8 {
    var limit = @min(text.len, 60);
    if (limit < text.len) {
        while (limit > 0 and (text[limit] & 0xC0) == 0x80) {
            limit -= 1;
        }
    }
    const result = try allocator.alloc(u8, limit);
    for (text[0..limit], 0..) |c, i| {
        result[i] = if (c == '\n' or c == '\r') ' ' else c;
    }
    return result;
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

fn emit_idea_json(writer: *std.Io.Writer, meta: IdeaMeta, first: bool) !void {
    if (!first) {
        try writer.writeAll(",");
    }
    try writer.writeAll("{\"filename\":\"");
    try print_escaped_json(writer, meta.filename);
    try writer.writeAll("\",\"project\":\"");
    try print_escaped_json(writer, meta.project);
    try writer.writeAll("\",\"title\":\"");
    try print_escaped_json(writer, meta.title);
    try writer.print("\",\"timestamp\":{d}", .{meta.timestamp});
    if (meta.tags.len > 0) {
        try writer.writeAll(",\"tags\":\"");
        try print_escaped_json(writer, meta.tags);
        try writer.writeAll("\"");
    }
    try writer.writeAll("}");
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

    try writer.print("  {s:<16}  {s}{s}", .{ meta.project, display_title, ellipsis });
    if (meta.tags.len > 0) {
        try writer.print("  [{s}]", .{meta.tags});
    }
    try writer.writeAll("\n");
    try writer.print("           {s}\n", .{meta.filename});
}

fn escape_yaml_string(arena: std.mem.Allocator, value: []const u8) ![]const u8 {
    // If it doesn't contain backslash or double quote, return as-is (duped)
    if (std.mem.indexOfScalar(u8, value, '\\') == null and std.mem.indexOfScalar(u8, value, '"') == null) {
        return try arena.dupe(u8, value);
    }
    var result: std.ArrayList(u8) = .empty;
    for (value) |c| {
        if (c == '\\' or c == '"') {
            try result.append(arena, '\\');
        }
        try result.append(arena, c);
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
            if (next == '"' or next == '\\') {
                try result.append(arena, next);
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

    var project: ?[]const u8 = null;
    var timestamp: ?i64 = null;
    var title: ?[]const u8 = null;
    var tags: ?[]const u8 = null;

    var pos = first_nl + 1;
    while (pos < content.len) {
        const line_end = std.mem.indexOfScalarPos(u8, content, pos, '\n') orelse content.len;
        const line = std.mem.trim(u8, content[pos..line_end], " \r");
        pos = line_end + 1;

        if (std.mem.eql(u8, line, "---")) break;

        const colon_idx = std.mem.indexOfScalar(u8, line, ':') orelse continue;
        const key = std.mem.trim(u8, line[0..colon_idx], " ");
        const value = std.mem.trim(u8, line[colon_idx + 1 ..], " ");

        if (std.mem.eql(u8, key, "project")) {
            project = try unescape_yaml_string(arena, value);
        } else if (std.mem.eql(u8, key, "title")) {
            title = try unescape_yaml_string(arena, value);
        } else if (std.mem.eql(u8, key, "timestamp")) {
            timestamp = std.fmt.parseInt(i64, value, 10) catch 0;
        } else if (std.mem.eql(u8, key, "tags")) {
            tags = try unescape_yaml_string(arena, value);
        }
    }

    return IdeaMeta{
        .filename = "",
        .project = project orelse "",
        .timestamp = timestamp orelse 0,
        .title = title orelse "",
        .tags = tags orelse "",
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

fn resolve_vault_path(arena: std.mem.Allocator, environ_map: anytype) []const u8 {
    if (environ_map.get("PIN_VAULT")) |custom| {
        if (custom.len > 0) return custom;
    }

    const home = environ_map.get("HOME") orelse {
        std.debug.print("Error: HOME environment variable is not set.\n", .{});
        std.process.exit(1);
    };
    return std.fs.path.join(arena, &.{ home, ".pin_vault" }) catch {
        std.debug.print("Error: Failed to construct vault path.\n", .{});
        std.process.exit(1);
    };
}

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

            // Tag filter
            if (filter_tag) |tag| {
                if (!tag_matches(raw_meta.tags, tag)) continue;
            }

            try list.append(arena, .{
                .filename = try arena.dupe(u8, entry.name),
                .project = raw_meta.project,
                .timestamp = raw_meta.timestamp,
                .title = raw_meta.title,
                .tags = raw_meta.tags,
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

fn print_usage() void {
    std.debug.print(
        \\Usage:
        \\  pin add "<markdown_content>" [--project <name>] [--title <string>] [--tags <comma,separated>]
        \\  pin add --stdin [--project <name>] [--title <string>] [--tags <comma,separated>]
        \\  pin list [--project <name>] [--tag <name>] [--format table]
        \\  pin list-project [--tag <name>] [--format table]
        \\  pin search "<query>" [--project <name>] [--tag <name>] [--format table]
        \\  pin read <filename>
        \\  pin rm <filename>
        \\  pin edit <filename>
        \\  pin stats
        \\
        \\Environment:
        \\  PIN_VAULT    Override default vault path (~/.pin_vault)
        \\
    , .{});
}

pub fn main(init: std.process.Init) !void {
    const io = init.io;
    const arena = init.arena.allocator();

    const args = try init.minimal.args.toSlice(arena);
    if (args.len < 2) {
        print_usage();
        std.process.exit(1);
    }

    const cmd = args[1];
    const vault_path = resolve_vault_path(arena, init.environ_map);

    // ── add ──────────────────────────────────────────────────────────────
    if (std.mem.eql(u8, cmd, "add")) {
        var content: ?[]const u8 = null;
        var project: ?[]const u8 = null;
        var title: ?[]const u8 = null;
        var tags: ?[]const u8 = null;
        var use_stdin = false;

        var idx: usize = 2;
        while (idx < args.len) : (idx += 1) {
            const arg = args[idx];
            if (std.mem.eql(u8, arg, "--project")) {
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: --project requires a value\n", .{});
                    std.process.exit(1);
                }
                idx += 1;
                project = args[idx];
            } else if (std.mem.eql(u8, arg, "--title")) {
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: --title requires a value\n", .{});
                    std.process.exit(1);
                }
                idx += 1;
                title = args[idx];
            } else if (std.mem.eql(u8, arg, "--tags")) {
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: --tags requires a value\n", .{});
                    std.process.exit(1);
                }
                idx += 1;
                tags = args[idx];
            } else if (std.mem.eql(u8, arg, "--stdin")) {
                use_stdin = true;
            } else {
                if (content != null) {
                    std.debug.print("Error: Multiple content arguments provided\n", .{});
                    std.process.exit(1);
                }
                content = arg;
            }
        }

        // Read from stdin if --stdin flag or no content arg and stdin is piped
        if (use_stdin or content == null) {
            if (try read_stdin(arena, io)) |stdin_content| {
                content = stdin_content;
            }
        }

        const final_content = content orelse {
            std.debug.print("Error: Content argument is required for 'add'\n", .{});
            std.process.exit(1);
        };

        // Resolve project name: flag > cwd basename
        var cwd_buf: [std.fs.max_path_bytes]u8 = undefined;
        const cwd_len = std.process.currentPath(io, &cwd_buf) catch |err| {
            std.debug.print("Error: Failed to get current working directory: {any}\n", .{err});
            std.process.exit(1);
        };
        const cwd_basename = std.fs.path.basename(cwd_buf[0..cwd_len]);

        const proj_name = if (project) |p| p else cwd_basename;
        const title_val = if (title) |t| try arena.dupe(u8, t) else try get_default_title(arena, final_content);

        const escaped_proj = try escape_yaml_string(arena, proj_name);
        const escaped_title = try escape_yaml_string(arena, title_val);
        const escaped_tags = if (tags) |t| try escape_yaml_string(arena, t) else null;

        // Get timestamp with nanosecond precision for unique filenames
        const ts = std.Io.Timestamp.now(io, .real);
        const seconds = ts.toSeconds();
        // Extract sub-second nanoseconds for filename uniqueness
        const total_ns = ts.toNanoseconds();
        const sub_ns: u64 = @intCast(@mod(total_ns, std.time.ns_per_s));

        const epoch_secs = std.time.epoch.EpochSeconds{ .secs = @intCast(seconds) };
        const epoch_day = epoch_secs.getEpochDay();
        const year_day = epoch_day.calculateYearDay();
        const month_day = year_day.calculateMonthDay();

        const year = year_day.year;
        const month = @intFromEnum(month_day.month);
        const day = month_day.day_index + 1;

        // Ensure vault directory exists
        std.Io.Dir.cwd().createDirPath(io, vault_path) catch |err| {
            std.debug.print("Error: Failed to create vault directory at '{s}': {any}\n", .{ vault_path, err });
            std.process.exit(1);
        };

        var dir = std.Io.Dir.openDirAbsolute(io, vault_path, .{}) catch |err| {
            std.debug.print("Error: Failed to open vault directory: {any}\n", .{err});
            std.process.exit(1);
        };
        defer dir.close(io);

        // Filename: YYYY-MM-DD_SECONDS_NANOS.md — unique even within the same second
        const filename = try std.fmt.allocPrint(arena, "{d:0>4}-{d:0>2}-{d:0>2}_{d}_{d}.md", .{ year, month, day, seconds, sub_ns });

        // YAML Front Matter — all string values quoted to avoid YAML parsing issues
        var file_content: []const u8 = undefined;
        if (escaped_tags) |et| {
            file_content = try std.fmt.allocPrint(arena,
                \\---
                \\project: "{s}"
                \\timestamp: {d}
                \\title: "{s}"
                \\tags: "{s}"
                \\---
                \\{s}
                \\
            , .{ escaped_proj, seconds, escaped_title, et, final_content });
        } else {
            file_content = try std.fmt.allocPrint(arena,
                \\---
                \\project: "{s}"
                \\timestamp: {d}
                \\title: "{s}"
                \\---
                \\{s}
                \\
            , .{ escaped_proj, seconds, escaped_title, final_content });
        }

        dir.writeFile(io, .{ .sub_path = filename, .data = file_content }) catch |err| {
            std.debug.print("Error: Failed to write to file '{s}': {any}\n", .{ filename, err });
            std.process.exit(1);
        };

        var out_buf: [256]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        try w.interface.print("Saved idea to {s}\n", .{filename});
        try w.end();

        // ── list ─────────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "list") or std.mem.eql(u8, cmd, "list-project")) {
        var filter_project: ?[]const u8 = null;
        var filter_tag: ?[]const u8 = null;
        var format_table = false;

        // list-project: default to cwd basename
        if (std.mem.eql(u8, cmd, "list-project")) {
            var cwd_buf: [std.fs.max_path_bytes]u8 = undefined;
            const cwd_len = std.process.currentPath(io, &cwd_buf) catch |err| {
                std.debug.print("Error: Failed to get current working directory: {any}\n", .{err});
                std.process.exit(1);
            };
            filter_project = try arena.dupe(u8, std.fs.path.basename(cwd_buf[0..cwd_len]));
        }

        // Parse flags
        var idx: usize = 2;
        while (idx < args.len) : (idx += 1) {
            const arg = args[idx];
            if (std.mem.eql(u8, arg, "--project")) {
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: --project requires a value\n", .{});
                    std.process.exit(1);
                }
                idx += 1;
                filter_project = args[idx];
            } else if (std.mem.eql(u8, arg, "--tag")) {
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: --tag requires a value\n", .{});
                    std.process.exit(1);
                }
                idx += 1;
                filter_tag = args[idx];
            } else if (std.mem.eql(u8, arg, "--format")) {
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: --format requires a value\n", .{});
                    std.process.exit(1);
                }
                idx += 1;
                if (std.mem.eql(u8, args[idx], "table")) {
                    format_table = true;
                } else if (!std.mem.eql(u8, args[idx], "json")) {
                    std.debug.print("Error: --format must be 'json' or 'table'\n", .{});
                    std.process.exit(1);
                }
            } else {
                std.debug.print("Error: Unknown flag '{s}'\n", .{arg});
                std.process.exit(1);
            }
        }

        var dir = open_vault_dir(io, vault_path, true) orelse {
            var out_buf: [32]u8 = undefined;
            var w = std.Io.File.stdout().writer(io, &out_buf);
            if (format_table) {
                try w.interface.print("No ideas found.\n", .{});
            } else {
                try w.interface.print("[]\n", .{});
            }
            try w.end();
            return;
        };
        defer dir.close(io);

        const ideas = try collect_ideas(arena, io, dir, filter_project, null, filter_tag);

        var out_buf: [4096]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);

        if (format_table) {
            if (ideas.len == 0) {
                try w.interface.print("No ideas found.\n", .{});
            } else {
                try w.interface.print("DATE        PROJECT           TITLE\n", .{});
                try w.interface.print("----------  ----------------  ----------------------------------------\n", .{});
                for (ideas) |meta| {
                    try emit_idea_table(&w.interface, meta);
                }
                try w.interface.print("\n{d} idea(s)\n", .{ideas.len});
            }
        } else {
            try w.interface.print("[", .{});
            for (ideas, 0..) |meta, i| {
                try emit_idea_json(&w.interface, meta, i == 0);
            }
            try w.interface.print("]\n", .{});
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
        var format_table = false;

        var idx: usize = 3;
        while (idx < args.len) : (idx += 1) {
            const arg = args[idx];
            if (std.mem.eql(u8, arg, "--project")) {
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: --project requires a value\n", .{});
                    std.process.exit(1);
                }
                idx += 1;
                filter_project = args[idx];
            } else if (std.mem.eql(u8, arg, "--tag")) {
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: --tag requires a value\n", .{});
                    std.process.exit(1);
                }
                idx += 1;
                filter_tag = args[idx];
            } else if (std.mem.eql(u8, arg, "--format")) {
                if (idx + 1 >= args.len) {
                    std.debug.print("Error: --format requires a value\n", .{});
                    std.process.exit(1);
                }
                idx += 1;
                if (std.mem.eql(u8, args[idx], "table")) {
                    format_table = true;
                } else if (!std.mem.eql(u8, args[idx], "json")) {
                    std.debug.print("Error: --format must be 'json' or 'table'\n", .{});
                    std.process.exit(1);
                }
            } else {
                std.debug.print("Error: Unknown flag '{s}'\n", .{arg});
                std.process.exit(1);
            }
        }

        var dir = open_vault_dir(io, vault_path, true) orelse {
            var out_buf: [32]u8 = undefined;
            var w = std.Io.File.stdout().writer(io, &out_buf);
            if (format_table) {
                try w.interface.print("No ideas found.\n", .{});
            } else {
                try w.interface.print("[]\n", .{});
            }
            try w.end();
            return;
        };
        defer dir.close(io);

        const ideas = try collect_ideas(arena, io, dir, filter_project, query, filter_tag);

        var out_buf: [4096]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);

        if (format_table) {
            if (ideas.len == 0) {
                try w.interface.print("No results for \"{s}\".\n", .{query});
            } else {
                try w.interface.print("DATE        PROJECT           TITLE\n", .{});
                try w.interface.print("----------  ----------------  ----------------------------------------\n", .{});
                for (ideas) |meta| {
                    try emit_idea_table(&w.interface, meta);
                }
                try w.interface.print("\n{d} result(s) for \"{s}\"\n", .{ ideas.len, query });
            }
        } else {
            try w.interface.print("[", .{});
            for (ideas, 0..) |meta, i| {
                try emit_idea_json(&w.interface, meta, i == 0);
            }
            try w.interface.print("]\n", .{});
        }
        try w.end();

        // ── read ─────────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "read")) {
        if (args.len < 3) {
            std.debug.print("Error: 'read' subcommand requires a filename argument\n", .{});
            std.process.exit(1);
        }
        const filename = args[2];

        if (!validate_filename(filename)) {
            std.debug.print("Error: Invalid filename '{s}'. Must be a plain filename, no paths.\n", .{filename});
            std.process.exit(1);
        }

        var dir = std.Io.Dir.openDirAbsolute(io, vault_path, .{}) catch |err| {
            if (err == error.FileNotFound) {
                std.debug.print("Error: File '{s}' not found in vault.\n", .{filename});
            } else {
                std.debug.print("Error: Failed to open vault directory: {any}\n", .{err});
            }
            std.process.exit(1);
        };
        defer dir.close(io);

        const content = dir.readFileAlloc(io, filename, arena, .unlimited) catch |err| {
            if (err == error.FileNotFound) {
                std.debug.print("Error: File '{s}' not found in vault.\n", .{filename});
            } else {
                std.debug.print("Error: Failed to read file '{s}': {any}\n", .{ filename, err });
            }
            std.process.exit(1);
        };

        const stdout_file = std.Io.File.stdout();
        stdout_file.writeStreamingAll(io, content) catch |err| {
            std.debug.print("Error: Failed to write to stdout: {any}\n", .{err});
            std.process.exit(1);
        };

        // ── rm ───────────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "rm")) {
        if (args.len < 3) {
            std.debug.print("Error: 'rm' subcommand requires a filename argument\n", .{});
            std.process.exit(1);
        }
        const filename = args[2];

        if (!validate_filename(filename)) {
            std.debug.print("Error: Invalid filename '{s}'. Must be a plain filename, no paths.\n", .{filename});
            std.process.exit(1);
        }

        var dir = std.Io.Dir.openDirAbsolute(io, vault_path, .{}) catch |err| {
            if (err == error.FileNotFound) {
                std.debug.print("Error: File '{s}' not found in vault.\n", .{filename});
            } else {
                std.debug.print("Error: Failed to open vault directory: {any}\n", .{err});
            }
            std.process.exit(1);
        };
        defer dir.close(io);

        dir.deleteFile(io, filename) catch |err| {
            if (err == error.FileNotFound) {
                std.debug.print("Error: File '{s}' not found in vault.\n", .{filename});
            } else {
                std.debug.print("Error: Failed to delete file '{s}': {any}\n", .{ filename, err });
            }
            std.process.exit(1);
        };

        var out_buf: [256]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        try w.interface.print("Removed {s}\n", .{filename});
        try w.end();

        // ── edit ─────────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "edit")) {
        if (args.len < 3) {
            std.debug.print("Error: 'edit' subcommand requires a filename argument\n", .{});
            std.process.exit(1);
        }
        const filename = args[2];

        if (!validate_filename(filename)) {
            std.debug.print("Error: Invalid filename '{s}'. Must be a plain filename, no paths.\n", .{filename});
            std.process.exit(1);
        }

        // Verify file exists
        {
            var dir = std.Io.Dir.openDirAbsolute(io, vault_path, .{}) catch |err| {
                if (err == error.FileNotFound) {
                    std.debug.print("Error: Vault not found.\n", .{});
                } else {
                    std.debug.print("Error: Failed to open vault directory: {any}\n", .{err});
                }
                std.process.exit(1);
            };
            defer dir.close(io);

            var file = dir.openFile(io, filename, .{ .mode = .read_only }) catch |err| {
                if (err == error.FileNotFound) {
                    std.debug.print("Error: File '{s}' not found in vault.\n", .{filename});
                } else {
                    std.debug.print("Error: Failed to open file '{s}': {any}\n", .{ filename, err });
                }
                std.process.exit(1);
            };
            file.close(io);
        }

        const full_path = try std.fs.path.join(arena, &.{ vault_path, filename });

        const editor = init.environ_map.get("EDITOR") orelse
            init.environ_map.get("VISUAL") orelse
            "vi";

        var child = std.process.spawn(io, .{
            .argv = &.{ editor, full_path },
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

        // ── stats ────────────────────────────────────────────────────────
    } else if (std.mem.eql(u8, cmd, "stats")) {
        var dir = open_vault_dir(io, vault_path, true) orelse {
            var out_buf: [128]u8 = undefined;
            var w = std.Io.File.stdout().writer(io, &out_buf);
            try w.interface.print("Vault:    {s}\nIdeas:    0\n", .{vault_path});
            try w.end();
            return;
        };
        defer dir.close(io);

        const ideas = try collect_ideas(arena, io, dir, null, null, null);

        var total: usize = 0;
        var oldest: i64 = std.math.maxInt(i64);
        var newest: i64 = 0;

        // Count unique projects and tags
        var projects: std.ArrayList([]const u8) = .empty;
        var tag_list: std.ArrayList([]const u8) = .empty;

        for (ideas) |meta| {
            total += 1;
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

        try w.interface.print("Vault:    {s}\n", .{vault_path});
        try w.interface.print("Ideas:    {d}\n", .{total});
        try w.interface.print("Projects: {d}\n", .{projects.items.len});

        if (projects.items.len > 0) {
            try w.interface.print("          ", .{});
            for (projects.items, 0..) |p, i| {
                if (i > 0) try w.interface.print(", ", .{});
                try w.interface.print("{s}", .{p});
            }
            try w.interface.print("\n", .{});
        }

        if (tag_list.items.len > 0) {
            try w.interface.print("Tags:     {d}\n", .{tag_list.items.len});
            try w.interface.print("          ", .{});
            for (tag_list.items, 0..) |t, i| {
                if (i > 0) try w.interface.print(", ", .{});
                try w.interface.print("{s}", .{t});
            }
            try w.interface.print("\n", .{});
        }

        if (total > 0) {
            try w.interface.print("Oldest:   ", .{});
            try format_date(&w.interface, oldest);
            try w.interface.print("\n", .{});
            try w.interface.print("Newest:   ", .{});
            try format_date(&w.interface, newest);
            try w.interface.print("\n", .{});
        }

        try w.end();
    } else {
        std.debug.print("Error: Unknown subcommand '{s}'\n\n", .{cmd});
        print_usage();
        std.process.exit(1);
    }
}
