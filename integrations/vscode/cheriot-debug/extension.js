// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

const vscode = require("vscode");

function activate(context) {
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory("cheriot", {
      createDebugAdapterDescriptor(session) {
        const folder = session.workspaceFolder ? session.workspaceFolder.uri.fsPath : undefined;
        return new vscode.DebugAdapterExecutable(
          session.configuration.cheriot || "cheriot",
          ["debug", "--dap"],
          { cwd: folder }
        );
      },
    })
  );
}

module.exports = { activate, deactivate() {} };
