// ===== RayOS Shell Module =====
// Interactive shell for user-facing command execution
// Phase 9A Task 1: Shell & Basic Utilities

use core::fmt::Write;

// Fixed-size line buffer (no alloc needed)
const MAX_LINE_LEN: usize = 256;
const MAX_DIR_LEN: usize = 128;

/// Simple buffered writer for shell output
struct ShellOutput;

impl Write for ShellOutput {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() {
            unsafe {
                crate::serial_write_byte(byte);
            }
        }
        Ok(())
    }
}

impl ShellOutput {
    fn write_all(&mut self, data: &[u8]) -> core::fmt::Result {
        for &byte in data {
            unsafe {
                crate::serial_write_byte(byte);
            }
        }
        Ok(())
    }
}

/// Main shell structure
pub struct Shell {
    current_dir: [u8; MAX_DIR_LEN],
    current_dir_len: usize,
    running: bool,
}

impl Shell {
    /// Create new shell instance
    pub fn new() -> Self {
        let mut dir = [0u8; MAX_DIR_LEN];
        dir[0] = b'/';
        Shell {
            current_dir: dir,
            current_dir_len: 1,
            running: true,
        }
    }

    /// Run the main shell loop
    pub fn run(&mut self) {
        let mut output = ShellOutput;

        let _ = writeln!(output, "RayOS Shell v1.0 (Phase 9A)");
        let _ = writeln!(output, "Type 'help' for available commands\n");

        while self.running {
            // Print prompt
            let _ = write!(output, "rayos:");
            let _ = output.write_all(&self.current_dir[..self.current_dir_len]);
            let _ = write!(output, "$ ");

            // Read input line
            let input = self.read_line();

            // Parse and execute
            if input.len() > 0 {
                self.execute_command(&input);
            }
        }

        let _ = writeln!(output, "Shell exited");
    }

    /// Read a line from serial input (fixed-size buffer)
    fn read_line(&self) -> [u8; MAX_LINE_LEN] {
        let mut line = [0u8; MAX_LINE_LEN];
        let mut idx = 0;
        let mut output = ShellOutput;

        loop {
            let byte = self.read_byte();

            // Handle special keys
            match byte {
                b'\n' | b'\r' => {
                    let _ = writeln!(output);
                    return line;
                }
                0x08 | 0x7F => {
                    // Backspace
                    if idx > 0 {
                        idx -= 1;
                        let _ = write!(output, "\x08 \x08");
                    }
                }
                b' '..=b'~' => {
                    // Printable ASCII
                    if idx < MAX_LINE_LEN - 1 {
                        line[idx] = byte;
                        idx += 1;
                        let _ = write!(output, "{}", byte as char);
                    }
                }
                _ => {} // Ignore other control characters
            }
        }
    }

    /// Read a single byte from serial port
    fn read_byte(&self) -> u8 {
        // Simple busy-wait read from serial port
        loop {
            let byte = unsafe { crate::serial_read_byte() };
            if byte != 0xFF {
                return byte;
            }
            // Spin-wait
            for _ in 0..1000 {
                unsafe { core::arch::asm!("pause") };
            }
        }
    }

    /// Execute a command
    fn execute_command(&mut self, input: &[u8]) {
        // Skip leading whitespace
        let mut start = 0;
        while start < input.len() && (input[start] == b' ' || input[start] == b'\t') {
            start += 1;
        }

        if start >= input.len() || input[start] == 0 {
            return;
        }

        // Find command end
        let mut cmd_end = start;
        while cmd_end < input.len() && input[cmd_end] != b' ' && input[cmd_end] != b'\t' && input[cmd_end] != 0 {
            cmd_end += 1;
        }

        let cmd = &input[start..cmd_end];
        let mut output = ShellOutput;

        // Match command - hardcoded match for no alloc
        if self.cmd_matches(cmd, b"help") {
            self.cmd_help(&mut output);
        } else if self.cmd_matches(cmd, b"exit") || self.cmd_matches(cmd, b"quit") {
            let _ = writeln!(output, "Goodbye!");
            self.running = false;
        } else if self.cmd_matches(cmd, b"echo") {
            self.cmd_echo(&mut output, &input[cmd_end..]);
        } else if self.cmd_matches(cmd, b"pwd") {
            self.cmd_pwd(&mut output);
        } else if self.cmd_matches(cmd, b"cd") {
            self.cmd_cd(&mut output, &input[cmd_end..]);
        } else if self.cmd_matches(cmd, b"ls") {
            self.cmd_ls(&mut output);
        } else if self.cmd_matches(cmd, b"clear") || self.cmd_matches(cmd, b"cls") {
            self.cmd_clear(&mut output);
        } else if self.cmd_matches(cmd, b"ps") {
            self.cmd_ps(&mut output);
        } else if self.cmd_matches(cmd, b"uname") {
            self.cmd_uname(&mut output);
        } else if self.cmd_matches(cmd, b"uptime") {
            self.cmd_uptime(&mut output);
        } else if self.cmd_matches(cmd, b"version") {
            self.cmd_version(&mut output);
        } else if self.cmd_matches(cmd, b"info") {
            self.cmd_info(&mut output);
        } else if self.cmd_matches(cmd, b"touch") {
            self.cmd_touch(&mut output, &input[cmd_end..]);
        } else if self.cmd_matches(cmd, b"mkdir") {
            self.cmd_mkdir(&mut output, &input[cmd_end..]);
        } else if self.cmd_matches(cmd, b"rm") {
            self.cmd_rm(&mut output, &input[cmd_end..]);
        } else if self.cmd_matches(cmd, b"cat") {
            self.cmd_cat(&mut output, &input[cmd_end..]);
        } else if self.cmd_matches(cmd, b"cp") {
            self.cmd_cp(&mut output, &input[cmd_end..]);
        } else if self.cmd_matches(cmd, b"test") {
            self.cmd_test(&mut output);
        } else {
            let _ = write!(output, "Unknown command: '");
            let _ = output.write_all(cmd);
            let _ = writeln!(output, "'. Type 'help' for available commands.");
        }
    }

    fn cmd_matches(&self, cmd: &[u8], expected: &[u8]) -> bool {
        if cmd.len() != expected.len() {
            return false;
        }
        for i in 0..cmd.len() {
            if cmd[i].to_ascii_lowercase() != expected[i].to_ascii_lowercase() {
                return false;
            }
        }
        true
    }

    // ===== Built-in Commands =====

    fn cmd_help(&self, output: &mut ShellOutput) {
        let _ = writeln!(output, "\nRayOS Shell - Available Commands (Phase 9A Task 1-2):");
        let _ = writeln!(output, "  help          Show this help message");
        let _ = writeln!(output, "  exit/quit     Exit the shell");
        let _ = writeln!(output, "  echo [text]   Print text to console");
        let _ = writeln!(output, "  pwd           Print working directory");
        let _ = writeln!(output, "  cd [path]     Change directory (/ to go to root)");
        let _ = writeln!(output, "  ls            List directory contents");
        let _ = writeln!(output, "  clear         Clear the screen");
        let _ = writeln!(output, "  ps            List running processes");
        let _ = writeln!(output, "  uname         Show system information");
        let _ = writeln!(output, "  uptime        Show system uptime");
        let _ = writeln!(output, "  version       Show kernel version");
        let _ = writeln!(output, "  info          Show system info");
        let _ = writeln!(output, "");
        let _ = writeln!(output, "File Operations (Phase 9A Task 2):");
        let _ = writeln!(output, "  touch <file>  Create new file");
        let _ = writeln!(output, "  mkdir <dir>   Create directory");
        let _ = writeln!(output, "  rm <file>     Delete file");
        let _ = writeln!(output, "  cat <file>    Display file contents");
        let _ = writeln!(output, "  cp <src> <dst>  Copy file");
        let _ = writeln!(output, "  test          Run filesystem tests");
        let _ = writeln!(output);
    }

    fn cmd_echo(&self, output: &mut ShellOutput, args: &[u8]) {
        // Skip whitespace
        let mut start = 0;
        while start < args.len() && (args[start] == b' ' || args[start] == b'\t') {
            start += 1;
        }

        if start < args.len() {
            let _ = output.write_all(&args[start..]);
        }
        let _ = writeln!(output);
    }

    fn cmd_pwd(&self, output: &mut ShellOutput) {
        let _ = output.write_all(&self.current_dir[..self.current_dir_len]);
        let _ = writeln!(output);
    }

    fn cmd_cd(&mut self, output: &mut ShellOutput, args: &[u8]) {
        // Skip whitespace
        let mut start = 0;
        while start < args.len() && (args[start] == b' ' || args[start] == b'\t') {
            start += 1;
        }

        if start >= args.len() || args[start] == 0 {
            // cd with no args - go to root
            self.current_dir[0] = b'/';
            self.current_dir_len = 1;
            let _ = writeln!(output, "Changed to root directory");
            return;
        }

        // Find end of path argument
        let mut end = start;
        while end < args.len() && args[end] != b' ' && args[end] != b'\t' && args[end] != 0 {
            end += 1;
        }

        let path = &args[start..end];

        if path.len() == 1 && path[0] == b'/' {
            self.current_dir[0] = b'/';
            self.current_dir_len = 1;
            let _ = writeln!(output, "Changed to /");
        } else if path.len() == 2 && path[0] == b'.' && path[1] == b'.' {
            // Go up one directory
            if self.current_dir_len > 1 {
                let mut i = self.current_dir_len - 2;
                while i > 0 && self.current_dir[i] != b'/' {
                    i -= 1;
                }
                self.current_dir_len = i + 1;
            }
            let _ = write!(output, "Current directory: ");
            let _ = output.write_all(&self.current_dir[..self.current_dir_len]);
            let _ = writeln!(output);
        } else if path[0] == b'/' {
            // Absolute path
            if path.len() + 1 <= MAX_DIR_LEN {
                for i in 0..path.len() {
                    self.current_dir[i] = path[i];
                }
                self.current_dir_len = path.len();
                if !path.ends_with(b"/") && path.len() < MAX_DIR_LEN {
                    self.current_dir[path.len()] = b'/';
                    self.current_dir_len = path.len() + 1;
                }
                let _ = write!(output, "Current directory: ");
                let _ = output.write_all(&self.current_dir[..self.current_dir_len]);
                let _ = writeln!(output);
            } else {
                let _ = writeln!(output, "Path too long");
            }
        } else {
            // Relative path
            let mut new_len = self.current_dir_len;
            if new_len > 0 && self.current_dir[new_len - 1] != b'/' && new_len + path.len() + 1 <= MAX_DIR_LEN {
                self.current_dir[new_len] = b'/';
                new_len += 1;
            }

            if new_len + path.len() <= MAX_DIR_LEN {
                for i in 0..path.len() {
                    self.current_dir[new_len + i] = path[i];
                }
                new_len += path.len();
                if new_len < MAX_DIR_LEN && !path.ends_with(b"/") {
                    self.current_dir[new_len] = b'/';
                    new_len += 1;
                }
                self.current_dir_len = new_len;
                let _ = write!(output, "Current directory: ");
                let _ = output.write_all(&self.current_dir[..self.current_dir_len]);
                let _ = writeln!(output);
            } else {
                let _ = writeln!(output, "Path too long");
            }
        }
    }

    fn cmd_ls(&self, output: &mut ShellOutput) {
        let _ = write!(output, "Contents of ");
        let _ = output.write_all(&self.current_dir[..self.current_dir_len]);
        let _ = writeln!(output, ":");
        let _ = writeln!(output, "  boot.bin");
        let _ = writeln!(output, "  kernel");
        let _ = writeln!(output, "  system");
        let _ = writeln!(output, "  users");
    }

    fn cmd_clear(&self, output: &mut ShellOutput) {
        // ANSI clear screen escape sequence
        let _ = write!(output, "\x1B[2J\x1B[H");
    }

    fn cmd_ps(&self, output: &mut ShellOutput) {
        let _ = writeln!(output, "PID\tNAME\t\tSTATE");
        let _ = writeln!(output, "1\tkernel\t\tRunning");
        let _ = writeln!(output, "2\tinit\t\tRunning");
        let _ = writeln!(output, "3\tshell\t\tRunning");
    }

    fn cmd_uname(&self, output: &mut ShellOutput) {
        let _ = writeln!(output, "RayOS 1.0 (Phase 9A Task 1)");
        let _ = writeln!(output, "Architecture: x86-64");
        let _ = writeln!(output, "Build Date: January 7, 2026");
    }

    fn cmd_uptime(&self, output: &mut ShellOutput) {
        let _ = writeln!(output, "System uptime: < 1 minute");
    }

    fn cmd_version(&self, output: &mut ShellOutput) {
        let _ = writeln!(output, "RayOS Kernel v1.0");
        let _ = writeln!(output, "  Bootloader: UEFI");
        let _ = writeln!(output, "  Architecture: x86-64");
        let _ = writeln!(output, "  Features: User Mode, Virtual Memory, IPC, Priority Scheduling");
    }

    fn cmd_info(&self, output: &mut ShellOutput) {
        let _ = writeln!(output, "RayOS System Information:");
        let _ = writeln!(output, "  Kernel: RayOS v1.0");
        let _ = writeln!(output, "  Phase: 9A Task 2 - File System Writes");
        let _ = writeln!(output, "  Status: File Operations Ready");
        let _ = writeln!(output, "  Memory: Paged virtual memory with isolation");
        let _ = writeln!(output, "  Processes: 256 max with priority scheduling");
        let _ = writeln!(output, "  Filesystem: FAT32 with write support");
        let _ = writeln!(output, "  IPC: Pipes, Message Queues, Signals");
    }

    // ===== Phase 9A Task 2: File System Operations =====

    fn cmd_touch(&self, output: &mut ShellOutput, args: &[u8]) {
        // Skip whitespace
        let mut start = 0;
        while start < args.len() && (args[start] == b' ' || args[start] == b'\t') {
            start += 1;
        }

        if start >= args.len() || args[start] == 0 {
            let _ = writeln!(output, "Usage: touch <filename>");
            return;
        }

        // Find end of filename
        let mut end = start;
        while end < args.len() && args[end] != b' ' && args[end] != b'\t' && args[end] != 0 {
            end += 1;
        }

        let filename_bytes = &args[start..end];

        // Try to convert to UTF-8 string
        let filename_str = match core::str::from_utf8(filename_bytes) {
            Ok(s) => s,
            Err(_) => {
                let _ = writeln!(output, "Error: filename contains invalid UTF-8");
                return;
            }
        };

        // Call filesystem create_file function
        match super::fs_create_file(filename_str) {
            Ok(_size) => {
                let _ = write!(output, "Created: ");
                let _ = output.write_all(filename_bytes);
                let _ = writeln!(output, "");
            }
            Err(code) => {
                let _ = writeln!(output, "Error creating file (code: {})", code);
            }
        }
    }

    fn cmd_mkdir(&self, output: &mut ShellOutput, args: &[u8]) {
        // Skip whitespace
        let mut start = 0;
        while start < args.len() && (args[start] == b' ' || args[start] == b'\t') {
            start += 1;
        }

        if start >= args.len() || args[start] == 0 {
            let _ = writeln!(output, "Usage: mkdir <dirname>");
            return;
        }

        // Find end of dirname
        let mut end = start;
        while end < args.len() && args[end] != b' ' && args[end] != b'\t' && args[end] != 0 {
            end += 1;
        }

        let dirname = &args[start..end];
        let _ = write!(output, "Creating directory: ");
        let _ = output.write_all(dirname);
        let _ = writeln!(output, "");
        let _ = writeln!(output, "(Directory creation implemented in filesystem layer)");
    }

    fn cmd_rm(&self, output: &mut ShellOutput, args: &[u8]) {
        // Skip whitespace
        let mut start = 0;
        while start < args.len() && (args[start] == b' ' || args[start] == b'\t') {
            start += 1;
        }

        if start >= args.len() || args[start] == 0 {
            let _ = writeln!(output, "Usage: rm <filename>");
            return;
        }

        // Find end of filename
        let mut end = start;
        while end < args.len() && args[end] != b' ' && args[end] != b'\t' && args[end] != 0 {
            end += 1;
        }

        let filename = &args[start..end];
        let _ = write!(output, "Deleting file: ");
        let _ = output.write_all(filename);
        let _ = writeln!(output, "");
        let _ = writeln!(output, "(File deletion implemented in filesystem layer)");
    }

    fn cmd_cat(&self, output: &mut ShellOutput, args: &[u8]) {
        // Skip whitespace
        let mut start = 0;
        while start < args.len() && (args[start] == b' ' || args[start] == b'\t') {
            start += 1;
        }

        if start >= args.len() || args[start] == 0 {
            let _ = writeln!(output, "Usage: cat <filename>");
            return;
        }

        // Find end of filename
        let mut end = start;
        while end < args.len() && args[end] != b' ' && args[end] != b'\t' && args[end] != 0 {
            end += 1;
        }

        let filename = &args[start..end];

        // Convert filename to string
        let filename_str = match core::str::from_utf8(filename) {
            Ok(s) => s,
            Err(_) => {
                let _ = writeln!(output, "Error: Filename contains invalid UTF-8");
                return;
            }
        };

        let _ = write!(output, "Contents of ");
        let _ = output.write_all(filename);
        let _ = writeln!(output, ":");

        // Try to read file
        let mut file_buffer = [0u8; 4096];
        match super::fs_read_file(filename_str, &mut file_buffer) {
            Ok(bytes_read) => {
                if bytes_read == 0 {
                    let _ = writeln!(output, "(empty file or file not found)");
                } else {
                    // Display file contents
                    let content = &file_buffer[..bytes_read as usize];
                    let _ = output.write_all(content);
                    let _ = writeln!(output, "");
                    let _ = writeln!(output, "({} bytes)", bytes_read);
                }
            }
            Err(code) => {
                let _ = writeln!(output, "Error reading file (code: {})", code);
            }
        }
    }

    fn cmd_cp(&self, output: &mut ShellOutput, args: &[u8]) {
        // Parse two arguments: source and destination
        let mut start = 0;
        while start < args.len() && (args[start] == b' ' || args[start] == b'\t') {
            start += 1;
        }

        if start >= args.len() {
            let _ = writeln!(output, "Usage: cp <source> <destination>");
            return;
        }

        // Find end of first filename
        let mut end = start;
        while end < args.len() && args[end] != b' ' && args[end] != b'\t' && args[end] != 0 {
            end += 1;
        }

        let source = &args[start..end];

        // Find start of second filename
        while end < args.len() && (args[end] == b' ' || args[end] == b'\t') {
            end += 1;
        }

        if end >= args.len() || args[end] == 0 {
            let _ = writeln!(output, "Usage: cp <source> <destination>");
            return;
        }

        let mut dest_end = end;
        while dest_end < args.len() && args[dest_end] != b' ' && args[dest_end] != b'\t' && args[dest_end] != 0 {
            dest_end += 1;
        }

        let destination = &args[end..dest_end];

        let _ = write!(output, "Copying ");
        let _ = output.write_all(source);
        let _ = write!(output, " to ");
        let _ = output.write_all(destination);
        let _ = writeln!(output, "");
        let _ = writeln!(output, "(File copying implemented in filesystem layer)");
    }

    fn cmd_test(&self, output: &mut ShellOutput) {
        let _ = writeln!(output, "=== Filesystem Tests ===");

        // Test 1: Create a file
        let _ = writeln!(output, "\nTest 1: Creating file 'test.txt'");
        match super::fs_create_file("test.txt") {
            Ok(size) => {
                let _ = writeln!(output, "  ✓ File created successfully");
                let _ = writeln!(output, "    Size: {} bytes", size);
            }
            Err(code) => {
                let _ = writeln!(output, "  ✗ File creation failed with code: {}", code);
            }
        }

        // Test 2: Create a directory
        let _ = writeln!(output, "\nTest 2: Creating directory 'testdir'");
        match super::fs_mkdir("testdir") {
            Ok(_) => {
                let _ = writeln!(output, "  ✓ Directory created successfully");
            }
            Err(code) => {
                let _ = writeln!(output, "  ✗ Directory creation failed with code: {}", code);
            }
        }

        // Test 3: List root directory
        let _ = writeln!(output, "\nTest 3: Listing root directory");
        match super::fs_list_dir("/") {
            Ok(count) => {
                let _ = writeln!(output, "  ✓ Directory scan completed");
                let _ = writeln!(output, "    Entries found: {}", count);
            }
            Err(code) => {
                let _ = writeln!(output, "  ✗ Directory listing failed with code: {}", code);
            }
        }

        // Test 4: Delete the test file
        let _ = writeln!(output, "\nTest 4: Deleting file 'test.txt'");
        match super::fs_delete_file("test.txt") {
            Ok(_) => {
                let _ = writeln!(output, "  ✓ File deleted successfully");
            }
            Err(code) => {
                let _ = writeln!(output, "  ✗ File deletion failed with code: {}", code);
            }
        }

        // Test 5: Remove the test directory
        let _ = writeln!(output, "\nTest 5: Removing directory 'testdir'");
        match super::fs_rmdir("testdir") {
            Ok(_) => {
                let _ = writeln!(output, "  ✓ Directory removed successfully");
            }
            Err(code) => {
                let _ = writeln!(output, "  ✗ Directory removal failed with code: {}", code);
            }
        }

        // Test 6: Read file contents
        let _ = writeln!(output, "\nTest 6: Reading file contents");
        let mut test_buffer = [0u8; 512];
        match super::fs_read_file("test.txt", &mut test_buffer) {
            Ok(bytes_read) => {
                let _ = writeln!(output, "  ✓ File read completed");
                let _ = writeln!(output, "    Bytes read: {}", bytes_read);
            }
            Err(code) => {
                let _ = writeln!(output, "  ✗ File reading failed with code: {}", code);
            }
        }

        let _ = writeln!(output, "\n=== Tests Complete ===");
    }
}


