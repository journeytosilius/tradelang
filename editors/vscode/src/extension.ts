import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    Trace,
    TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
    const restartCommand = vscode.commands.registerCommand(
        "palmscript.restartLanguageServer",
        async () => {
            await restartClient(context);
        },
    );
    context.subscriptions.push(restartCommand);

    const configWatcher = vscode.workspace.createFileSystemWatcher("**/.palmscript.json");
    const restartOnChange = async (): Promise<void> => {
        if (client !== undefined) {
            await restartClient(context);
        }
    };
    configWatcher.onDidChange(restartOnChange);
    configWatcher.onDidCreate(restartOnChange);
    configWatcher.onDidDelete(restartOnChange);
    context.subscriptions.push(configWatcher);

    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration(async (event) => {
            if (
                event.affectsConfiguration("palmscript.server.path") ||
                event.affectsConfiguration("palmscript.projectConfigPath") ||
                event.affectsConfiguration("palmscript.trace.server")
            ) {
                await restartClient(context);
            }
        }),
    );

    await startClient(context);
}

export async function deactivate(): Promise<void> {
    if (client !== undefined) {
        await client.stop();
        client = undefined;
    }
}

async function restartClient(context: vscode.ExtensionContext): Promise<void> {
    if (client !== undefined) {
        await client.stop();
        client = undefined;
    }
    await startClient(context);
}

async function startClient(context: vscode.ExtensionContext): Promise<void> {
    const serverPath = await resolveServerBinary(context);
    const serverOptions: ServerOptions = {
        run: {
            command: serverPath,
            transport: TransportKind.stdio,
        },
        debug: {
            command: serverPath,
            transport: TransportKind.stdio,
        },
    };
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: "file", language: "palmscript" }],
        initializationOptions: {
            projectConfigPath: vscode.workspace
                .getConfiguration("palmscript")
                .get<string>("projectConfigPath", ".palmscript.json"),
        },
    };

    client = new LanguageClient(
        "palmscript",
        "PalmScript Language Server",
        serverOptions,
        clientOptions,
    );
    await client.start();
    await client.setTrace(traceLevel());
}

async function resolveServerBinary(context: vscode.ExtensionContext): Promise<string> {
    const configured = vscode.workspace
        .getConfiguration("palmscript")
        .get<string>("server.path", "")
        .trim();
    if (configured.length > 0) {
        if (!fs.existsSync(configured)) {
            throw new Error(`Configured PalmScript server not found: ${configured}`);
        }
        return configured;
    }

    const bundled = bundledBinaryPath(context);
    if (bundled !== undefined) {
        return bundled;
    }

    const devFallbacks = [
        path.resolve(context.extensionPath, "..", "..", "target", "debug", binaryName()),
        path.resolve(context.extensionPath, "..", "..", "target", "release", binaryName()),
    ];
    const resolved = devFallbacks.find((candidate) => fs.existsSync(candidate));
    if (resolved !== undefined) {
        return resolved;
    }

    throw new Error(
        "Could not find palmscript-lsp. Configure `palmscript.server.path` or build the repo binary.",
    );
}

function bundledBinaryPath(context: vscode.ExtensionContext): string | undefined {
    const platform = process.platform;
    const arch = process.arch;
    const candidate = path.join(
        context.extensionPath,
        "server",
        `${platform}-${arch}`,
        binaryName(),
    );
    if (fs.existsSync(candidate)) {
        return candidate;
    }
    return undefined;
}

function binaryName(): string {
    return process.platform === "win32" ? "palmscript-lsp.exe" : "palmscript-lsp";
}

function traceLevel(): Trace {
    const trace = vscode.workspace
        .getConfiguration("palmscript")
        .get<string>("trace.server", "off");
    switch (trace) {
        case "messages":
            return Trace.Messages;
        case "verbose":
            return Trace.Verbose;
        default:
            return Trace.Off;
    }
}
