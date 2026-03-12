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
let outputChannel: vscode.OutputChannel | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
    outputChannel = vscode.window.createOutputChannel("PalmScript");
    context.subscriptions.push(outputChannel);
    const restartCommand = vscode.commands.registerCommand(
        "palmscript.restartLanguageServer",
        async () => {
            await restartClient(context);
        },
    );
    context.subscriptions.push(restartCommand);

    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration(async (event) => {
            if (
                event.affectsConfiguration("palmscript.server.path") ||
                event.affectsConfiguration("palmscript.trace.server")
            ) {
                await restartClient(context);
            }
        }),
    );

    await ensureClientStarted(context);
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
    await ensureClientStarted(context);
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
    };

    client = new LanguageClient(
        "palmscript",
        "PalmScript Language Server",
        serverOptions,
        clientOptions,
    );
    await client.start();
    await client.setTrace(traceLevel());
    outputChannel?.appendLine(`PalmScript language server started: ${serverPath}`);
}

async function ensureClientStarted(context: vscode.ExtensionContext): Promise<void> {
    try {
        await startClient(context);
    } catch (error) {
        outputChannel?.appendLine(startupFailureMessage(error));
        if (error instanceof Error && error.stack !== undefined) {
            outputChannel?.appendLine(error.stack);
        }
        const action = await vscode.window.showErrorMessage(
            startupFailureMessage(error),
            "Open Settings",
            "Show Output",
        );
        if (action === "Open Settings") {
            await vscode.commands.executeCommand(
                "workbench.action.openSettings",
                "palmscript.server.path",
            );
        } else if (action === "Show Output") {
            outputChannel?.show(true);
        }
    }
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
    for (const name of bundledBinaryNames()) {
        const candidate = path.join(context.extensionPath, "server", `${platform}-${arch}`, name);
        if (fs.existsSync(candidate)) {
            return candidate;
        }
    }
    return undefined;
}

function binaryName(): string {
    return process.platform === "win32" ? "palmscript-lsp.exe" : "palmscript-lsp";
}

function bundledBinaryNames(): string[] {
    if (process.platform === "win32") {
        return ["palmscript-lsp.exe"];
    }
    return ["palmscript-lsp", "tradelang-lsp"];
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

export function startupFailureMessage(error: unknown): string {
    const details = error instanceof Error ? error.message : String(error);
    return `PalmScript language features are unavailable because palmscript-lsp could not be started: ${details}`;
}

export const testHooks = {
    bundledBinaryNames,
};
