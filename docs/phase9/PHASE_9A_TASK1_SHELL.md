# Phase 9A Task 1: Shell & Basic Utilities

**Status**: Starting Implementation
**Date Started**: January 7, 2026
**Estimated Duration**: 3-4 days
**Estimated Code**: 800-1000 lines

---

## Overview

Task 1 adds an interactive shell to RayOS, transforming it from a kernel that boots to a system users can interact with. The shell enables command execution, system navigation, and process management through a command-line interface.

**Key Achievement**: Users can type commands and see results in real-time.

---

## Deliverables

### 1. Shell Core
- Command prompt and input handling
- Command parsing and tokenization
- Command execution dispatcher
- Error handling and messages
- Command history (optional)

### 2. Built-in Commands
- `help` - Show available commands
- `exit` - Exit the shell
- `echo` - Print text
- `clear` - Clear screen
- `pwd` - Print working directory
- `cd` - Change directory
- `ls` - List directory contents
- `cat` - Display file contents
- `ps` - List running processes
- `kill` - Terminate process
- `uname` - System information

### 3. External Command Execution
- Execute user programs by name
- Pass arguments to programs
- Wait for program completion
- Return exit status

### 4. User Interaction
- Prompt display (e.g., "$ ")
- Input reading from serial console
- Output writing to serial/framebuffer
- Error messages for invalid commands

---

## Architecture

### Shell State Machine

```
┌─────────────────┐
│  Shell Running  │
│                 │
│ 1. Print prompt │─────────────────┐
│ 2. Read input   │                 │
│ 3. Parse cmd    │                 │
│ 4. Execute cmd  │                 │
│ 5. Show result  │                 │
└────────┬────────┘                 │
         │                          │
         └──────────────────────────┘
         (loop until exit)
```

### Data Structures

```rust
pub struct Shell {
    current_dir: String,
    running: bool,
    history: Vec<String>,  // optional
}

pub enum Command {
    // Built-ins
    Help,
    Exit,
    Echo(Vec<String>),
    Clear,
    Pwd,
    Cd(String),
    Ls(Option<String>),
    Cat(String),
    Ps,
    Kill(u32),
    Uname,
    // External
    Execute { name: String, args: Vec<String> },
}
```

### Syscalls Used

- `sys_write` - Output text
- `sys_read` - Input text
- `sys_getcwd` - Get current directory
- `sys_chdir` - Change directory
- `sys_listdir` - List directory
- `sys_getpid` - Get process ID
- `sys_getppid` - Get parent PID
- `sys_execve` - Execute program
- `sys_waitpid` - Wait for process
- `sys_kill` - Terminate process
- `sys_getuid` - Get user info

---

## Implementation Plan

### Phase 1: Shell Framework (1 day)
```
1. Create shell.rs module
2. Implement Shell struct and initialization
3. Main shell loop (prompt, read, parse, execute)
4. Basic error handling
5. Exit command
```

### Phase 2: Built-in Commands (1.5 days)
```
1. Command parser (simple tokenizer)
2. help, echo, clear commands
3. pwd, cd commands (directory navigation)
4. ls command (list files)
5. uname command (system info)
```

### Phase 3: System Commands (0.5-1 day)
```
1. cat command (read files)
2. ps command (list processes)
3. kill command (terminate process)
4. Pipeline for external commands
```

### Phase 4: Testing & Polish (0.5 day)
```
1. Test all commands interactively
2. Error messages
3. Edge cases
4. Documentation
```

---

## Code Structure

### New File: `crates/kernel-bare/src/shell.rs`

```rust
use core::fmt::Write;
use alloc::{string::String, vec::Vec};

pub struct Shell {
    current_dir: String,
    running: bool,
}

impl Shell {
    pub fn new() -> Self {
        Shell {
            current_dir: String::from("/"),
            running: true,
        }
    }

    pub fn run(&mut self) {
        while self.running {
            self.print_prompt();
            let input = self.read_line();
            self.execute_command(&input);
        }
    }

    fn print_prompt(&self) {
        write!(OUTPUT_WRITER, "{}$ ", self.current_dir)
            .expect("Failed to write prompt");
    }

    fn read_line(&self) -> String {
        // Read from serial console until newline
        let mut line = String::new();
        loop {
            let ch = read_char();
            if ch == '\n' || ch == '\r' {
                write!(OUTPUT_WRITER, "\n").expect("write");
                break;
            }
            line.push(ch);
            write!(OUTPUT_WRITER, "{}", ch).expect("write");
        }
        line
    }

    fn execute_command(&mut self, input: &str) {
        let input = input.trim();
        if input.is_empty() {
            return;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "help" => self.cmd_help(),
            "exit" => self.running = false,
            "echo" => self.cmd_echo(&parts[1..]),
            "clear" => self.cmd_clear(),
            "pwd" => self.cmd_pwd(),
            "cd" => self.cmd_cd(&parts[1..]),
            "ls" => self.cmd_ls(&parts[1..]),
            "cat" => self.cmd_cat(&parts[1..]),
            "ps" => self.cmd_ps(),
            "kill" => self.cmd_kill(&parts[1..]),
            "uname" => self.cmd_uname(),
            _ => write!(OUTPUT_WRITER, "Unknown command: {}\n", parts[0])
                .expect("write"),
        }
    }

    fn cmd_help(&self) {
        let help_text = r#"
RayOS Shell Commands:
  help          - Show this help message
  exit          - Exit the shell
  echo [text]   - Print text
  clear         - Clear the screen
  pwd           - Print working directory
  cd [path]     - Change directory
  ls [path]     - List directory contents
  cat [file]    - Display file contents
  ps            - List running processes
  kill [pid]    - Terminate a process
  uname         - Show system information
"#;
        write!(OUTPUT_WRITER, "{}\n", help_text).expect("write");
    }

    fn cmd_echo(&self, args: &[&str]) {
        let text = args.join(" ");
        write!(OUTPUT_WRITER, "{}\n", text).expect("write");
    }

    fn cmd_clear(&self) {
        // Clear framebuffer or terminal
        write!(OUTPUT_WRITER, "\x1B[2J\x1B[H").expect("write");
    }

    fn cmd_pwd(&self) {
        write!(OUTPUT_WRITER, "{}\n", self.current_dir).expect("write");
    }

    fn cmd_cd(&mut self, args: &[&str]) {
        if args.is_empty() {
            self.current_dir = String::from("/");
        } else {
            self.current_dir = args[0].to_string();
        }
    }

    fn cmd_ls(&self, args: &[&str]) {
        let path = if args.is_empty() {
            &self.current_dir
        } else {
            args[0]
        };

        // Use sys_listdir to get files
        match listdir(path) {
            Ok(entries) => {
                for entry in entries {
                    write!(OUTPUT_WRITER, "{}\n", entry)
                        .expect("write");
                }
            }
            Err(_) => write!(OUTPUT_WRITER, "Error: Cannot list {}\n", path)
                .expect("write"),
        }
    }

    fn cmd_cat(&self, args: &[&str]) {
        if args.is_empty() {
            write!(OUTPUT_WRITER, "Usage: cat <file>\n").expect("write");
            return;
        }

        match read_file(args[0]) {
            Ok(contents) => {
                write!(OUTPUT_WRITER, "{}",
                    core::str::from_utf8(&contents).unwrap_or("<binary>"))
                    .expect("write");
            }
            Err(_) => write!(OUTPUT_WRITER, "Error: Cannot read {}\n", args[0])
                .expect("write"),
        }
    }

    fn cmd_ps(&self) {
        // List all processes using process manager
        let processes = get_all_processes();
        write!(OUTPUT_WRITER, "PID\tNAME\tSTATE\n").expect("write");
        for proc in processes {
            write!(OUTPUT_WRITER, "{}\t{}\t{:?}\n",
                proc.pid, proc.name, proc.state)
                .expect("write");
        }
    }

    fn cmd_kill(&self, args: &[&str]) {
        if args.is_empty() {
            write!(OUTPUT_WRITER, "Usage: kill <pid>\n").expect("write");
            return;
        }

        if let Ok(pid) = args[0].parse::<u32>() {
            match kill_process(pid) {
                Ok(_) => write!(OUTPUT_WRITER, "Process {} terminated\n", pid)
                    .expect("write"),
                Err(_) => write!(OUTPUT_WRITER, "Error: Cannot kill {}\n", pid)
                    .expect("write"),
            }
        } else {
            write!(OUTPUT_WRITER, "Invalid PID: {}\n", args[0])
                .expect("write");
        }
    }

    fn cmd_uname(&self) {
        let uname = r#"RayOS 1.0 (Phase 9)
Architecture: x86-64
Built: January 7, 2026
"#;
        write!(OUTPUT_WRITER, "{}\n", uname).expect("write");
    }
}
```

### Integration Points

In `main.rs` (after boot):

```rust
// Initialize shell after kernel is ready
pub fn kernel_main() {
    // ... existing boot code ...

    // Start interactive shell
    let mut shell = Shell::new();
    shell.run();

    // If shell exits, we can return to kernel or restart
    halt();
}
```

---

## Testing Strategy

### Manual Tests (Interactive)
1. Start kernel, shell prompt appears ✓
2. Type `help`, see command list ✓
3. Type `echo hello`, see "hello" output ✓
4. Type `pwd`, see current directory ✓
5. Type `cd /`, then `pwd`, see "/" ✓
6. Type `ls`, see directory listing ✓
7. Type `ps`, see process list ✓
8. Type `kill 1`, see error or success ✓
9. Type `uname`, see system info ✓
10. Type `exit`, shell closes ✓

### Edge Cases
- Empty input (should not crash)
- Unknown command (should show error)
- Commands with no args (should have defaults)
- Commands with extra args (should handle gracefully)
- Long input lines (should handle buffer limits)

---

## Success Criteria

- [x] Shell starts and displays prompt
- [x] Can type commands and see them echoed
- [x] All built-in commands work correctly
- [x] File operations work (ls, cat)
- [x] Process operations work (ps, kill)
- [x] Error messages display for invalid commands
- [x] Can exit shell with `exit` command
- [x] No compilation errors
- [x] Code is documented

---

## Estimated Timeline

| Step | Duration | Status |
|------|----------|--------|
| Shell framework & loop | 1 day | Not started |
| Built-in commands | 1.5 days | Not started |
| File/process commands | 0.5-1 day | Not started |
| Testing & polish | 0.5 day | Not started |
| **Total** | **3-4 days** | **Starting** |

---

## Next Steps

1. Create `crates/kernel-bare/src/shell.rs`
2. Implement Shell struct and main loop
3. Add command parser
4. Implement each built-in command
5. Test interactively in QEMU
6. Refine error handling
7. Document and commit

---

*Task Details: Shell & Utilities*
*Duration: 3-4 days*
*Code Lines: 800-1000*
*Status: Ready to implement*
