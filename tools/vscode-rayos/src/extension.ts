import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';

/**
 * RayOS VS Code Extension
 *
 * Provides development tools for building RayOS applications:
 * - Build commands for kernel and apps
 * - QEMU integration for testing
 * - Code snippets for App SDK
 * - Syntax highlighting for .rayapp manifests
 */

let outputChannel: vscode.OutputChannel;
let statusBarItem: vscode.StatusBarItem;

export function activate(context: vscode.ExtensionContext) {
    console.log('RayOS Development extension activated');

    // Create output channel for build logs
    outputChannel = vscode.window.createOutputChannel('RayOS');

    // Create status bar item
    statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
    statusBarItem.text = '$(rocket) RayOS';
    statusBarItem.tooltip = 'RayOS Development Tools';
    statusBarItem.command = 'rayos.showMenu';
    statusBarItem.show();
    context.subscriptions.push(statusBarItem);

    // Register commands
    context.subscriptions.push(
        vscode.commands.registerCommand('rayos.build', buildKernel),
        vscode.commands.registerCommand('rayos.run', runInQemu),
        vscode.commands.registerCommand('rayos.clean', cleanBuild),
        vscode.commands.registerCommand('rayos.createApp', createNewApp),
        vscode.commands.registerCommand('rayos.buildApp', buildCurrentApp),
        vscode.commands.registerCommand('rayos.showDocs', showDocumentation),
        vscode.commands.registerCommand('rayos.showMenu', showQuickPick)
    );

    // Register task provider
    context.subscriptions.push(
        vscode.tasks.registerTaskProvider('rayos', new RayOSTaskProvider())
    );

    // Show welcome message on first activation
    const hasShownWelcome = context.globalState.get('rayos.welcomeShown');
    if (!hasShownWelcome) {
        showWelcomeMessage();
        context.globalState.update('rayos.welcomeShown', true);
    }
}

export function deactivate() {
    if (outputChannel) {
        outputChannel.dispose();
    }
}

// =============================================================================
// Commands
// =============================================================================

async function buildKernel() {
    const workspaceFolder = getWorkspaceFolder();
    if (!workspaceFolder) {
        vscode.window.showErrorMessage('No workspace folder open');
        return;
    }

    const config = vscode.workspace.getConfiguration('rayos');
    const arch = config.get<string>('targetArch', 'x86_64');

    outputChannel.show();
    outputChannel.appendLine(`\n[RayOS] Building kernel for ${arch}...`);
    outputChannel.appendLine('─'.repeat(60));

    statusBarItem.text = '$(sync~spin) Building...';

    const kernelPath = path.join(workspaceFolder.uri.fsPath, 'crates', 'kernel-bare');
    const targetJson = `${arch}-rayos-kernel.json`;

    const task = new vscode.Task(
        { type: 'rayos', task: 'build-kernel' },
        workspaceFolder,
        'Build Kernel',
        'rayos',
        new vscode.ShellExecution(
            `cargo +nightly build --release --features ui_shell --target ${targetJson} -Z build-std=core,alloc -Z build-std-features=compiler-builtins-mem`,
            { cwd: kernelPath }
        ),
        '$rustc'
    );

    try {
        const execution = await vscode.tasks.executeTask(task);

        // Wait for task to complete
        const disposable = vscode.tasks.onDidEndTaskProcess(e => {
            if (e.execution === execution) {
                if (e.exitCode === 0) {
                    statusBarItem.text = '$(check) RayOS';
                    outputChannel.appendLine('\n[RayOS] Build succeeded!');
                    vscode.window.showInformationMessage('RayOS kernel built successfully');
                } else {
                    statusBarItem.text = '$(error) RayOS';
                    outputChannel.appendLine(`\n[RayOS] Build failed with exit code ${e.exitCode}`);
                    vscode.window.showErrorMessage('RayOS kernel build failed');
                }
                disposable.dispose();
            }
        });
    } catch (err) {
        statusBarItem.text = '$(error) RayOS';
        outputChannel.appendLine(`\n[RayOS] Build error: ${err}`);
        vscode.window.showErrorMessage(`Build error: ${err}`);
    }
}

async function runInQemu() {
    const workspaceFolder = getWorkspaceFolder();
    if (!workspaceFolder) {
        vscode.window.showErrorMessage('No workspace folder open');
        return;
    }

    const config = vscode.workspace.getConfiguration('rayos');
    const qemuPath = config.get<string>('qemuPath', 'qemu-system-x86_64');
    const enableSerial = config.get<boolean>('enableSerial', true);
    const extraArgs = config.get<string>('extraQemuArgs', '');

    outputChannel.show();
    outputChannel.appendLine('\n[RayOS] Starting QEMU...');
    outputChannel.appendLine('─'.repeat(60));

    const scriptPath = path.join(workspaceFolder.uri.fsPath, 'scripts', 'run-ui-shell.sh');

    if (!fs.existsSync(scriptPath)) {
        vscode.window.showErrorMessage('run-ui-shell.sh script not found');
        return;
    }

    const terminal = vscode.window.createTerminal({
        name: 'RayOS QEMU',
        cwd: workspaceFolder.uri.fsPath
    });

    terminal.show();
    terminal.sendText(`./scripts/run-ui-shell.sh ${extraArgs}`);

    statusBarItem.text = '$(vm-running) RayOS Running';
}

async function cleanBuild() {
    const workspaceFolder = getWorkspaceFolder();
    if (!workspaceFolder) {
        vscode.window.showErrorMessage('No workspace folder open');
        return;
    }

    outputChannel.show();
    outputChannel.appendLine('\n[RayOS] Cleaning build artifacts...');

    const kernelPath = path.join(workspaceFolder.uri.fsPath, 'crates', 'kernel-bare');

    const task = new vscode.Task(
        { type: 'rayos', task: 'clean' },
        workspaceFolder,
        'Clean',
        'rayos',
        new vscode.ShellExecution('cargo clean', { cwd: kernelPath }),
        []
    );

    await vscode.tasks.executeTask(task);
    outputChannel.appendLine('[RayOS] Clean complete');
    vscode.window.showInformationMessage('RayOS build cleaned');
}

async function createNewApp() {
    const appName = await vscode.window.showInputBox({
        prompt: 'Enter app name',
        placeHolder: 'my_app',
        validateInput: (value) => {
            if (!value || value.length === 0) {
                return 'App name is required';
            }
            if (!/^[a-z][a-z0-9_]*$/.test(value)) {
                return 'App name must start with lowercase letter and contain only a-z, 0-9, _';
            }
            return null;
        }
    });

    if (!appName) {
        return;
    }

    const workspaceFolder = getWorkspaceFolder();
    if (!workspaceFolder) {
        vscode.window.showErrorMessage('No workspace folder open');
        return;
    }

    // Generate app scaffold
    const appDir = path.join(workspaceFolder.uri.fsPath, 'apps', appName);

    if (fs.existsSync(appDir)) {
        vscode.window.showErrorMessage(`App '${appName}' already exists`);
        return;
    }

    fs.mkdirSync(appDir, { recursive: true });

    // Create main.rs
    const mainRs = generateAppTemplate(appName);
    fs.writeFileSync(path.join(appDir, 'main.rs'), mainRs);

    // Create .rayapp manifest
    const manifest = generateAppManifest(appName);
    fs.writeFileSync(path.join(appDir, `${appName}.rayapp`), manifest);

    // Open the main file
    const mainUri = vscode.Uri.file(path.join(appDir, 'main.rs'));
    await vscode.window.showTextDocument(mainUri);

    vscode.window.showInformationMessage(`Created new RayOS app: ${appName}`);
}

async function buildCurrentApp() {
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
        vscode.window.showErrorMessage('No active editor');
        return;
    }

    // For now, just build the kernel (apps are built-in)
    await buildKernel();
}

async function showDocumentation() {
    const workspaceFolder = getWorkspaceFolder();
    if (!workspaceFolder) {
        vscode.window.showErrorMessage('No workspace folder open');
        return;
    }

    const docsPath = path.join(workspaceFolder.uri.fsPath, 'docs');
    const docFiles = [
        { label: '$(book) App Development', file: 'development/APP_DEVELOPMENT.md' },
        { label: '$(rocket) Roadmap', file: 'ROADMAP.md' },
        { label: '$(list-tree) Process Explorer', file: 'PROCESS_EXPLORER.md' },
        { label: '$(output) System Log', file: 'SYSTEM_LOG.md' }
    ];

    const selection = await vscode.window.showQuickPick(docFiles, {
        placeHolder: 'Select documentation to open'
    });

    if (selection) {
        const docUri = vscode.Uri.file(path.join(docsPath, selection.file));
        if (fs.existsSync(docUri.fsPath)) {
            await vscode.commands.executeCommand('markdown.showPreview', docUri);
        } else {
            vscode.window.showErrorMessage(`Documentation not found: ${selection.file}`);
        }
    }
}

async function showQuickPick() {
    const items = [
        { label: '$(tools) Build Kernel', command: 'rayos.build' },
        { label: '$(play) Run in QEMU', command: 'rayos.run' },
        { label: '$(add) Create New App', command: 'rayos.createApp' },
        { label: '$(trash) Clean Build', command: 'rayos.clean' },
        { label: '$(book) Documentation', command: 'rayos.showDocs' }
    ];

    const selection = await vscode.window.showQuickPick(items, {
        placeHolder: 'RayOS Development'
    });

    if (selection) {
        vscode.commands.executeCommand(selection.command);
    }
}

function showWelcomeMessage() {
    vscode.window.showInformationMessage(
        'Welcome to RayOS Development! Use the status bar icon or Ctrl+Shift+P → "RayOS" to get started.',
        'Open Documentation'
    ).then(selection => {
        if (selection === 'Open Documentation') {
            showDocumentation();
        }
    });
}

// =============================================================================
// Task Provider
// =============================================================================

class RayOSTaskProvider implements vscode.TaskProvider {
    provideTasks(): vscode.ProviderResult<vscode.Task[]> {
        const workspaceFolder = getWorkspaceFolder();
        if (!workspaceFolder) {
            return [];
        }

        const tasks: vscode.Task[] = [];

        // Build kernel task
        tasks.push(new vscode.Task(
            { type: 'rayos', task: 'build-kernel' },
            workspaceFolder,
            'Build Kernel (x86_64)',
            'rayos',
            new vscode.ShellExecution(
                'cargo +nightly build --release --features ui_shell --target x86_64-rayos-kernel.json -Z build-std=core,alloc -Z build-std-features=compiler-builtins-mem',
                { cwd: path.join(workspaceFolder.uri.fsPath, 'crates', 'kernel-bare') }
            ),
            '$rustc'
        ));

        // Run task
        tasks.push(new vscode.Task(
            { type: 'rayos', task: 'run' },
            workspaceFolder,
            'Run in QEMU',
            'rayos',
            new vscode.ShellExecution('./scripts/run-ui-shell.sh'),
            []
        ));

        // Clean task
        tasks.push(new vscode.Task(
            { type: 'rayos', task: 'clean' },
            workspaceFolder,
            'Clean',
            'rayos',
            new vscode.ShellExecution('cargo clean', {
                cwd: path.join(workspaceFolder.uri.fsPath, 'crates', 'kernel-bare')
            }),
            []
        ));

        return tasks;
    }

    resolveTask(task: vscode.Task): vscode.ProviderResult<vscode.Task> {
        return task;
    }
}

// =============================================================================
// Helpers
// =============================================================================

function getWorkspaceFolder(): vscode.WorkspaceFolder | undefined {
    const folders = vscode.workspace.workspaceFolders;
    if (!folders || folders.length === 0) {
        return undefined;
    }
    return folders[0];
}

function generateAppTemplate(appName: string): string {
    const structName = appName.split('_')
        .map(word => word.charAt(0).toUpperCase() + word.slice(1))
        .join('');

    return `//! ${structName} - A RayOS Application
//!
//! This app was generated using the RayOS VS Code extension.

use crate::ui::app_sdk::{App, AppCapabilities, AppContext, AppDescriptor, AppEvent, MouseButton};

/// ${structName} application state.
pub struct ${structName} {
    // Add your app state here
    counter: u32,
}

impl ${structName} {
    /// App metadata.
    pub const DESCRIPTOR: AppDescriptor = AppDescriptor::new(b"${structName}", b"1.0.0")
        .with_author(b"Your Name")
        .with_description(b"Description of your app")
        .with_app_id(b"com.example.${appName}")
        .with_size(400, 300)
        .with_min_size(200, 150);

    /// Create a new instance.
    pub fn new() -> Self {
        Self {
            counter: 0,
        }
    }
}

impl App for ${structName} {
    fn descriptor() -> AppDescriptor {
        Self::DESCRIPTOR
    }

    fn on_init(&mut self, _ctx: &mut AppContext) {
        // Initialize your app here
    }

    fn on_frame(&mut self, ctx: &mut AppContext) {
        // Clear background
        ctx.clear(0x2A2A4E);

        // Draw title
        ctx.draw_text(20, 20, b"${structName}", 0xFFFFFF);

        // Draw content
        ctx.draw_text(20, 60, b"Hello from RayOS!", 0x88CCFF);

        // TODO: Add your rendering code here
    }

    fn on_event(&mut self, ctx: &mut AppContext, event: AppEvent) {
        match event {
            AppEvent::MouseDown { x, y, button: MouseButton::Left } => {
                // Handle mouse click
            }
            AppEvent::KeyDown { scancode, .. } => {
                // Handle key press
            }
            AppEvent::CloseRequested => {
                ctx.close();
            }
            _ => {}
        }
    }

    fn on_destroy(&mut self, _ctx: &mut AppContext) {
        // Cleanup code here
    }
}
`;
}

function generateAppManifest(appName: string): string {
    const structName = appName.split('_')
        .map(word => word.charAt(0).toUpperCase() + word.slice(1))
        .join('');

    return `# RayOS App Manifest
# Generated by RayOS VS Code Extension

[app]
name = "${structName}"
version = "1.0.0"
author = "Your Name"
description = "Description of your app"
app_id = "com.example.${appName}"

[window]
width = 400
height = 300
min_width = 200
min_height = 150
resizable = true

[capabilities]
# filesystem = true
# network = true
# clipboard = true

[build]
entry = "main.rs"
target = "rayos"
`;
}
