const std = @import("std");

const IdeaMeta = struct {
    project: []const u8,
    timestamp: i64,
    title: []const u8,
};

fn get_default_title(allocator: std.mem.Allocator, text: []const u8) ![]const u8 {
    var limit = @min(text.len, 30);
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
            }
        }
    }
}

fn parse_front_matter(
    arena: std.mem.Allocator,
    dir: std.Io.Dir,
    io: std.Io,
    filename: []const u8,
) !?IdeaMeta {
    var file = dir.openFile(io, filename, .{ .mode = .read_only }) catch |err| {
        if (err == error.FileNotFound) return null;
        return err;
    };
    defer file.close(io);

    var buf: [1024]u8 = undefined;
    var r = file.reader(io, &buf);

    // First line must be "---"
    const first_line = (try r.interface.takeDelimiter('\n')) orelse return null;
    const cleaned_first = std.mem.trim(u8, first_line, " \r");
    if (!std.mem.eql(u8, cleaned_first, "---")) return null;

    var project: ?[]const u8 = null;
    var timestamp: ?i64 = null;
    var title: ?[]const u8 = null;

    while (try r.interface.takeDelimiter('\n')) |line| {
        var cleaned = std.mem.trim(u8, line, " \r");
        if (std.mem.eql(u8, cleaned, "---")) {
            break;
        }

        const colon_idx = std.mem.indexOfScalar(u8, cleaned, ':') orelse continue;
        const key = std.mem.trim(u8, cleaned[0..colon_idx], " ");
        const value = std.mem.trim(u8, cleaned[colon_idx + 1..], " ");

        if (std.mem.eql(u8, key, "project")) {
            project = try arena.dupe(u8, value);
        } else if (std.mem.eql(u8, key, "title")) {
            var title_val = value;
            if (title_val.len >= 2 and title_val[0] == '"' and title_val[title_val.len - 1] == '"') {
                title_val = title_val[1 .. title_val.len - 1];
            } else if (title_val.len >= 2 and title_val[0] == '\'' and title_val[title_val.len - 1] == '\'') {
                title_val = title_val[1 .. title_val.len - 1];
            }
            title = try arena.dupe(u8, title_val);
        } else if (std.mem.eql(u8, key, "timestamp")) {
            timestamp = std.fmt.parseInt(i64, value, 10) catch 0;
        }
    }

    return IdeaMeta{
        .project = project orelse "",
        .timestamp = timestamp orelse 0,
        .title = title orelse "",
    };
}

fn print_usage() void {
    std.debug.print(
        \\Usage:
        \\  pin add "<detailed_markdown_content>" [--project <name>] [--title <string>]
        \\  pin list
        \\  pin list-project
        \\  pin search "<query>"
        \\  pin read <filename>
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
    
    // Resolve home directory and vault path
    const home = init.environ_map.get("HOME") orelse {
        std.debug.print("Error: HOME environment variable is not set.\n", .{});
        std.process.exit(1);
    };
    const vault_path = try std.fs.path.join(arena, &.{ home, ".pin_vault" });
    
    if (std.mem.eql(u8, cmd, "add")) {
        var content: ?[]const u8 = null;
        var project: ?[]const u8 = null;
        var title: ?[]const u8 = null;
        
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
            } else {
                if (content != null) {
                    std.debug.print("Error: Multiple content arguments provided\n", .{});
                    std.process.exit(1);
                }
                content = arg;
            }
        }
        
        const final_content = content orelse {
            std.debug.print("Error: Content argument is required for 'add'\n", .{});
            std.process.exit(1);
        };
        
        // Extract base name of current working directory
        var cwd_buf: [std.fs.max_path_bytes]u8 = undefined;
        const cwd_len = std.process.currentPath(io, &cwd_buf) catch |err| {
            std.debug.print("Error: Failed to get current working directory: {any}\n", .{err});
            std.process.exit(1);
        };
        const cwd_basename = std.fs.path.basename(cwd_buf[0..cwd_len]);
        
        const proj_name = if (project) |p| p else cwd_basename;
        const title_val = if (title) |t| try arena.dupe(u8, t) else try get_default_title(arena, final_content);
        
        // Get Unix Timestamp and format date
        const ts = std.Io.Timestamp.now(io, .real);
        const seconds = ts.toSeconds();
        
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
        
        // Open vault directory
        var dir = std.Io.Dir.openDirAbsolute(io, vault_path, .{}) catch |err| {
            std.debug.print("Error: Failed to open vault directory: {any}\n", .{err});
            std.process.exit(1);
        };
        defer dir.close(io);
        
        // Generate filename YYYY-MM-DD_UNIXTIMESTAMP.md
        const filename = try std.fmt.allocPrint(arena, "{d:0>4}-{d:0>2}-{d:0>2}_{d}.md", .{ year, month, day, seconds });
        
        // YAML Front Matter + Markdown body
        const file_content = try std.fmt.allocPrint(arena,
            \\---
            \\project: {s}
            \\timestamp: {d}
            \\title: {s}
            \\---
            \\{s}
            \\
        , .{ proj_name, seconds, title_val, final_content });
        
        dir.writeFile(io, .{ .sub_path = filename, .data = file_content }) catch |err| {
            std.debug.print("Error: Failed to write to file '{s}': {any}\n", .{ filename, err });
            std.process.exit(1);
        };
        
        // Print success to stderr (or we can use stdout for filename as well, but standard is stderr for info, let's write to stdout/stderr depending on preference. Here standard is to print to stdout)
        var out_buf: [128]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        try w.interface.print("Saved idea to {s}\n", .{filename});
        try w.end();
    } else if (std.mem.eql(u8, cmd, "list")) {
        var dir = std.Io.Dir.openDirAbsolute(io, vault_path, .{ .iterate = true }) catch |err| {
            if (err == error.FileNotFound) {
                var out_buf: [32]u8 = undefined;
                var w = std.Io.File.stdout().writer(io, &out_buf);
                try w.interface.print("[]\n", .{});
                try w.end();
                return;
            }
            std.debug.print("Error: Failed to open vault directory: {any}\n", .{err});
            std.process.exit(1);
        };
        defer dir.close(io);
        
        var it = dir.iterate();
        var first = true;
        
        var out_buf: [4096]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        try w.interface.print("[", .{});
        
        while (try it.next(io)) |entry| {
            if (entry.kind == .file and std.mem.endsWith(u8, entry.name, ".md")) {
                if (try parse_front_matter(arena, dir, io, entry.name)) |meta| {
                    if (!first) {
                        try w.interface.print(",", .{});
                    }
                    first = false;
                    
                    try w.interface.print("{{\"filename\":\"", .{});
                    try print_escaped_json(&w.interface, entry.name);
                    try w.interface.print("\",\"project\":\"", .{});
                    try print_escaped_json(&w.interface, meta.project);
                    try w.interface.print("\",\"title\":\"", .{});
                    try print_escaped_json(&w.interface, meta.title);
                    try w.interface.print("\",\"timestamp\":{d}}}", .{meta.timestamp});
                }
            }
        }
        
        try w.interface.print("]\n", .{});
        try w.end();
    } else if (std.mem.eql(u8, cmd, "list-project")) {
        // Extract base name of current working directory
        var cwd_buf: [std.fs.max_path_bytes]u8 = undefined;
        const cwd_len = std.process.currentPath(io, &cwd_buf) catch |err| {
            std.debug.print("Error: Failed to get current working directory: {any}\n", .{err});
            std.process.exit(1);
        };
        const cwd_basename = std.fs.path.basename(cwd_buf[0..cwd_len]);
        
        var dir = std.Io.Dir.openDirAbsolute(io, vault_path, .{ .iterate = true }) catch |err| {
            if (err == error.FileNotFound) {
                var out_buf: [32]u8 = undefined;
                var w = std.Io.File.stdout().writer(io, &out_buf);
                try w.interface.print("[]\n", .{});
                try w.end();
                return;
            }
            std.debug.print("Error: Failed to open vault directory: {any}\n", .{err});
            std.process.exit(1);
        };
        defer dir.close(io);
        
        var it = dir.iterate();
        var first = true;
        
        var out_buf: [4096]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        try w.interface.print("[", .{});
        
        while (try it.next(io)) |entry| {
            if (entry.kind == .file and std.mem.endsWith(u8, entry.name, ".md")) {
                if (try parse_front_matter(arena, dir, io, entry.name)) |meta| {
                    if (std.mem.eql(u8, meta.project, cwd_basename)) {
                        if (!first) {
                            try w.interface.print(",", .{});
                        }
                        first = false;
                        
                        try w.interface.print("{{\"filename\":\"", .{});
                        try print_escaped_json(&w.interface, entry.name);
                        try w.interface.print("\",\"project\":\"", .{});
                        try print_escaped_json(&w.interface, meta.project);
                        try w.interface.print("\",\"title\":\"", .{});
                        try print_escaped_json(&w.interface, meta.title);
                        try w.interface.print("\",\"timestamp\":{d}}}", .{meta.timestamp});
                    }
                }
            }
        }
        
        try w.interface.print("]\n", .{});
        try w.end();
    } else if (std.mem.eql(u8, cmd, "search")) {
        if (args.len < 3) {
            std.debug.print("Error: 'search' subcommand requires a query argument\n", .{});
            std.process.exit(1);
        }
        const query = args[2];
        
        var dir = std.Io.Dir.openDirAbsolute(io, vault_path, .{ .iterate = true }) catch |err| {
            if (err == error.FileNotFound) {
                var out_buf: [32]u8 = undefined;
                var w = std.Io.File.stdout().writer(io, &out_buf);
                try w.interface.print("[]\n", .{});
                try w.end();
                return;
            }
            std.debug.print("Error: Failed to open vault directory: {any}\n", .{err});
            std.process.exit(1);
        };
        defer dir.close(io);
        
        var it = dir.iterate();
        var first = true;
        
        var out_buf: [4096]u8 = undefined;
        var w = std.Io.File.stdout().writer(io, &out_buf);
        try w.interface.print("[", .{});
        
        while (try it.next(io)) |entry| {
            if (entry.kind == .file and std.mem.endsWith(u8, entry.name, ".md")) {
                const content = dir.readFileAlloc(io, entry.name, arena, .unlimited) catch continue;
                if (std.ascii.findIgnoreCase(content, query) != null) {
                    if (try parse_front_matter(arena, dir, io, entry.name)) |meta| {
                        if (!first) {
                            try w.interface.print(",", .{});
                        }
                        first = false;
                        
                        try w.interface.print("{{\"filename\":\"", .{});
                        try print_escaped_json(&w.interface, entry.name);
                        try w.interface.print("\",\"project\":\"", .{});
                        try print_escaped_json(&w.interface, meta.project);
                        try w.interface.print("\",\"title\":\"", .{});
                        try print_escaped_json(&w.interface, meta.title);
                        try w.interface.print("\",\"timestamp\":{d}}}", .{meta.timestamp});
                    }
                }
            }
        }
        
        try w.interface.print("]\n", .{});
        try w.end();
    } else if (std.mem.eql(u8, cmd, "read")) {
        if (args.len < 3) {
            std.debug.print("Error: 'read' subcommand requires a filename argument\n", .{});
            std.process.exit(1);
        }
        const filename = args[2];
        
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
                std.debug.print("Error: Failed to read file '{s}': {any}\n", .{filename, err});
            }
            std.process.exit(1);
        };
        
        const stdout_file = std.Io.File.stdout();
        stdout_file.writeStreamingAll(io, content) catch |err| {
            std.debug.print("Error: Failed to write to stdout: {any}\n", .{err});
            std.process.exit(1);
        };
    } else {
        std.debug.print("Error: Unknown subcommand '{s}'\n\n", .{cmd});
        print_usage();
        std.process.exit(1);
    }
}
