export function collectBrowserDiagnostics(page) {
  const diagnostics = {
    pageErrors: [],
    consoleErrors: [],
    badResponses: [],
  };
  page.on("pageerror", (error) => {
    diagnostics.pageErrors.push(String(error.message || error));
  });
  page.on("console", (message) => {
    if (message.type() === "error") {
      diagnostics.consoleErrors.push(message.text());
    }
  });
  page.on("response", (response) => {
    if (response.status() >= 400) {
      diagnostics.badResponses.push({
        url: response.url(),
        status: response.status(),
      });
    }
  });
  return diagnostics;
}

export function buildScenarioResult({ name, workspaceRoot, result, diagnostics }) {
  const { pageErrors, consoleErrors, badResponses } = diagnostics;
  return {
    ...result,
    name,
    workspaceRoot,
    pageErrors,
    consoleErrors,
    badResponses,
    // Scenario assertions cannot override failures observed by the browser.
    ok:
      result.ok === true
      && pageErrors.length === 0
      && consoleErrors.length === 0
      && badResponses.length === 0,
  };
}
