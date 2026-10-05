import { spawn } from "node:child_process";
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { buildScenarioResult, collectBrowserDiagnostics } from "./ui-smoke-result.mjs";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, "..");

const DEFAULT_HOST = "127.0.0.1";
const DEFAULT_PORT = 18992;
const REVIEW_ID = "ui_smoke::review_gate_apply_flow";
const REVIEW_TITLE = "UI smoke harness review";
const REVIEW_TEXT = {
  needsReview: ["needs review", "要レビュー", "à revoir"],
  approved: ["approved", "承認済み", "approuvé"],
  held: ["held", "保留中", "en attente"],
  applied: ["applied", "反映済み", "appliqué"],
};
const APPROVAL_PATHS = {
  edit: "notes/runtime-approval-edit.txt",
  commandApproved: "runtime-approval-command-approved.txt",
  commandRejected: "runtime-approval-command-rejected.txt",
};

function parseArgs(argv) {
  const parsed = {
    workspaceRoot: repoRoot,
    host: DEFAULT_HOST,
    port: DEFAULT_PORT,
    scenario: "all",
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--workspace-root" && argv[i + 1]) {
      parsed.workspaceRoot = path.resolve(argv[++i]);
    } else if (arg === "--host" && argv[i + 1]) {
      parsed.host = argv[++i];
    } else if (arg === "--port" && argv[i + 1]) {
      parsed.port = Number(argv[++i]) || parsed.port;
    } else if (arg === "--scenario" && argv[i + 1]) {
      parsed.scenario = String(argv[++i] || "all").trim().toLowerCase();
    }
  }
  return parsed;
}

function nowStamp() {
  return String(Date.now());
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForServer(url, timeoutMs = 20000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
    } catch (_) {
      // retry
    }
    await sleep(250);
  }
  throw new Error(`timed out waiting for ${url}`);
}

async function waitForCondition(predicate, description, timeoutMs = 12000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (await predicate()) return;
    await sleep(150);
  }
  throw new Error(`timed out waiting for ${description}`);
}

async function postJson(url, body) {
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    const text = await response.text().catch(() => "");
    throw new Error(`HTTP ${response.status} for ${url}: ${text}`);
  }
  return response.json();
}

function includesAny(text, candidates) {
  return candidates.some((candidate) => text.includes(candidate));
}

async function locatorContainsAny(locator, candidates) {
  try {
    const text = String(await locator.innerText()).toLowerCase();
    return includesAny(
      text,
      candidates.map((candidate) => String(candidate).toLowerCase())
    );
  } catch (_) {
    return false;
  }
}

async function ensureEnglish(page) {
  const enButton = page.getByRole("button", { name: /^EN$/ }).first();
  if (await enButton.count()) {
    await enButton.click();
    await page.waitForTimeout(150);
  }
}

async function isSelectorInViewport(page, selector) {
  return page.evaluate((query) => {
    const el = document.querySelector(query);
    if (!el) return false;
    const rect = el.getBoundingClientRect();
    return rect.top < window.innerHeight && rect.bottom > 0;
  }, selector);
}

async function captureViewport(page, url, name, viewport, outDir) {
  await page.setViewportSize(viewport);
  await page.goto(url, { waitUntil: "domcontentloaded", timeout: 30000 });
  await page.waitForSelector(".panel", { timeout: 15000 });
  await page.waitForSelector("h2", { timeout: 15000 });
  await page.waitForTimeout(1200);

  const headings = await page.locator("h2").allTextContents();
  const hasHorizontalOverflow = () => page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth + 4
  );
  const coderInput = page.getByRole("textbox", { name: /^(Coder|コーダー|Codeur)$/ });
  const minimumCoderWidth = Math.min(280, viewport.width * 0.7);
  const readCoderViewport = () => coderInput.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    return {
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
      inViewport:
        rect.width > 0 && rect.height > 0
        && rect.left >= 0 && rect.right <= window.innerWidth + 1
        && rect.top >= 0 && rect.bottom <= window.innerHeight + 1,
    };
  });
  const initialCoder = await readCoderViewport();
  const checks = {
    coderInitiallyInViewport: initialCoder.inViewport,
    coderInitiallyWideEnough: initialCoder.width >= minimumCoderWidth,
    horizontalOverflow: !(await hasHorizontalOverflow()),
  };

  // Capture the initial viewport before any control opens or scrolls the page.
  const screenshotPath = path.join(outDir, `${name}.png`);
  await page.screenshot({ path: screenshotPath, fullPage: false });

  const mobile = viewport.width <= 980;
  const sidebar = page.locator(".workspace-sidebar");
  const settingsToggle = page.getByRole("button", {
    name: /^(Settings · Threads|設定 · スレッド|Réglages · Fils)$/,
  });
  if (mobile) {
    checks.sidebarInitiallyHidden = !(await sidebar.isVisible());
    checks.settingsToggleVisible = await settingsToggle.isVisible();
    checks.settingsToggleInitiallyCollapsed = (await settingsToggle.getAttribute("aria-expanded")) === "false";
    await settingsToggle.click();
    await sidebar.waitFor({ state: "visible", timeout: 5000 });
    checks.settingsToggleExpanded = (await settingsToggle.getAttribute("aria-expanded")) === "true";
    checks.settingsOpenWithoutHorizontalOverflow = !(await hasHorizontalOverflow());
  }

  // These panels remain reachable on mobile through the settings control.
  checks.harnessReviews = await page.getByRole("heading", {
    name: /^(Harness reviews|ハーネスレビュー|Revues du harnais)$/,
  }).isVisible();
  checks.runtimeApprovals = await page.getByRole("heading", {
    name: /^(Runtime approvals|ランタイム承認|Approbations runtime)$/,
  }).isVisible();
  checks.settings = await page.getByRole("heading", {
    name: /^(Settings|設定|Réglages)$/,
  }).isVisible();

  let coderAfterSidebarClose = null;
  if (mobile) {
    await settingsToggle.click();
    await sidebar.waitFor({ state: "hidden", timeout: 5000 });
    checks.settingsToggleCollapsedAgain = (await settingsToggle.getAttribute("aria-expanded")) === "false";
    coderAfterSidebarClose = await readCoderViewport();
    checks.coderVisibleAfterSettingsClose = coderAfterSidebarClose.inViewport;
    checks.coderWideEnoughAfterSettingsClose = coderAfterSidebarClose.width >= minimumCoderWidth;
    checks.settingsClosedWithoutHorizontalOverflow = !(await hasHorizontalOverflow());
  }

  return {
    name,
    viewport,
    headings,
    screenshotPath,
    screenshotState: "initial-coder-viewport",
    minimumCoderWidth,
    initialCoder,
    coderAfterSidebarClose,
    checks,
  };
}

async function seedHarnessReviewWorkspace(outDir) {
  const root = path.join(outDir, "harness_review_workspace");
  await mkdir(path.join(root, ".spiral-coder"), { recursive: true });
  await mkdir(path.join(root, "shared"), { recursive: true });

  const contractPath = path.join(root, "shared", "governor_contract.json");
  await copyFile(path.join(repoRoot, "shared", "governor_contract.json"), contractPath);

  const proposedTemplate = {
    lane: "scaffold",
    artifact_mode: "new_repo",
    pattern: "repo_scaffold_drift",
    policy_action: "advance_repo_scaffold_artifact",
    required_action: "write_artifact",
    preferred_tools: ["write_file", "exec"],
    blocked_tools: ["list_dir", "read_file"],
    blocked_scope: "repo_scaffold",
    blocked_command_display: "list_dir demo_repo",
    next_target: "demo_repo/README.md",
    exit_hint: "continue",
    support_note: "Seeded by ui smoke review flow",
  };

  const candidatePath = path.join(root, ".spiral-coder", "governor_contract.promotion.json");
  const candidate = {
    version: 1,
    generated_at_ms: Date.now(),
    contract_path: "shared/governor_contract.json",
    overlay_path: ".spiral-coder/governor_contract.overlay.json",
    output_path: ".spiral-coder/governor_contract.promotion.json",
    summary: {
      total: 1,
      add: 1,
      update: 0,
      noop: 0,
      hold: 0,
      invalid: 0,
      eligible: 1,
      min_green_cases: 1,
    },
    candidates: [
      {
        id: REVIEW_ID,
        decision: "add",
        contract_path: `/runtime_overlay_templates/${REVIEW_ID}`,
        display: {
          title: REVIEW_TITLE,
          subtitle: "Promote seeded runtime overlay template for GUI smoke",
          badge: "add",
        },
        reasons: [
          "seeded GUI smoke candidate",
          "meets eval gate with 1 green case(s)",
        ],
        green_case_ids: ["ui-smoke-review-flow"],
        proposed_template: proposedTemplate,
        patch: {
          op: "add",
          path: `/runtime_overlay_templates/${REVIEW_ID}`,
          value: proposedTemplate,
        },
      },
    ],
  };

  await writeFile(candidatePath, `${JSON.stringify(candidate, null, 2)}\n`, "utf8");
  await writeFile(
    path.join(root, ".spiral-coder", "governor_contract.overlay.json"),
    `${JSON.stringify({ promoted_policies: [] }, null, 2)}\n`,
    "utf8"
  );

  return {
    root,
    contractPath,
    candidatePath,
    gatePath: path.join(root, ".spiral-coder", "governor_contract.promotion_gate.json"),
  };
}

async function seedRuntimeApprovalWorkspace(outDir) {
  const root = path.join(outDir, "runtime_approval_workspace");
  await mkdir(path.join(root, ".spiral-coder"), { recursive: true });
  await mkdir(path.join(root, "notes"), { recursive: true });
  return {
    root,
    editPath: path.join(root, APPROVAL_PATHS.edit),
    approvedCommandPath: path.join(root, APPROVAL_PATHS.commandApproved),
    rejectedCommandPath: path.join(root, APPROVAL_PATHS.commandRejected),
  };
}

async function runServerScenario(name, workspaceRoot, host, port, outDir, execute) {
  const serverArgs = [
    "run",
    "--quiet",
    "--manifest-path",
    path.join(repoRoot, "Cargo.toml"),
    "--",
    "serve",
    "--host",
    host,
    "--port",
    String(port),
  ];

  const server = spawn("cargo", serverArgs, {
    cwd: workspaceRoot,
    stdio: ["ignore", "pipe", "pipe"],
  });

  let serverStdout = "";
  let serverStderr = "";
  server.stdout.on("data", (chunk) => {
    serverStdout += chunk.toString();
  });
  server.stderr.on("data", (chunk) => {
    serverStderr += chunk.toString();
  });

  let browser;
  try {
    const baseUrl = `http://${host}:${port}/`;
    await waitForServer(baseUrl);

    browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();
    const diagnostics = collectBrowserDiagnostics(page);

    const result = await execute({
      page,
      baseUrl,
      workspaceRoot,
      outDir,
    });
    return buildScenarioResult({
      name,
      workspaceRoot,
      result,
      diagnostics,
    });
  } finally {
    if (browser) {
      await browser.close();
    }
    server.kill("SIGINT");
    await new Promise((resolve) => {
      server.once("exit", resolve);
      setTimeout(resolve, 5000);
    });
    if (serverStderr.trim()) {
      await writeFile(path.join(outDir, `${name}.server.stderr.log`), serverStderr, "utf8");
    }
    if (serverStdout.trim()) {
      await writeFile(path.join(outDir, `${name}.server.stdout.log`), serverStdout, "utf8");
    }
  }
}

async function runBaselineScenario(args, outDir) {
  return runServerScenario(
    "baseline",
    args.workspaceRoot,
    args.host,
    args.port,
    outDir,
    async ({ page, baseUrl }) => {
      const viewports = [
        { name: "desktop", viewport: { width: 1440, height: 900 } },
        { name: "mobile", viewport: { width: 390, height: 844 } },
      ];
      const results = [];
      for (const entry of viewports) {
        results.push(
          await captureViewport(page, baseUrl, entry.name, entry.viewport, outDir)
        );
      }
      return {
        kind: "baseline",
        results,
        ok: results.every((result) => Object.values(result.checks).every(Boolean)),
      };
    }
  );
}

async function runHarnessReviewFlowScenario(args, outDir) {
  const seeded = await seedHarnessReviewWorkspace(outDir);
  return runServerScenario(
    "harness-review-flow",
    seeded.root,
    args.host,
    args.port,
    outDir,
    async ({ page, baseUrl }) => {
      await page.setViewportSize({ width: 1440, height: 1800 });
      await page.goto(baseUrl, { waitUntil: "domcontentloaded", timeout: 30000 });
      await page.waitForSelector(".panel-review", { timeout: 15000 });
      await page.waitForTimeout(800);
      await ensureEnglish(page);

      const reviewJump = page.getByRole("button", { name: /Harness reviews/i }).first();
      await reviewJump.waitFor({ timeout: 15000 });
      const jumpButtonVisible = await reviewJump.isVisible();
      await reviewJump.click();
      await waitForCondition(
        () => isSelectorInViewport(page, ".panel-review"),
        "harness reviews panel to scroll into view"
      );

      const reviewPanel = page.locator(".panel-review");
      const reviewCard = reviewPanel.locator(".review-card").filter({ hasText: REVIEW_TITLE }).first();
      await reviewCard.waitFor({ timeout: 15000 });
      await waitForCondition(
        () => locatorContainsAny(reviewCard, REVIEW_TEXT.needsReview),
        "initial needs-review card"
      );

      await reviewCard.getByRole("button", { name: /^(Hold|保留|Mettre en attente)$/ }).click();
      await waitForCondition(
        () => locatorContainsAny(reviewCard, REVIEW_TEXT.held),
        "held review card"
      );

      await reviewCard.getByRole("button", { name: /^(Approve|承認|Approuver)$/ }).click();
      await waitForCondition(
        async () =>
          await locatorContainsAny(reviewCard, REVIEW_TEXT.approved)
          && await reviewCard
            .getByRole("button", { name: /^(Apply to contract|contractへ反映|Appliquer au contrat)$/ })
            .count() > 0,
        "approved review card"
      );

      await reviewCard
        .getByRole("button", { name: /^(Apply to contract|contractへ反映|Appliquer au contrat)$/ })
        .click();
      await waitForCondition(
        () => locatorContainsAny(reviewCard, REVIEW_TEXT.applied),
        "applied review card"
      );
      await waitForCondition(
        async () => {
          const actionButtons = reviewCard.locator("button");
          return (await actionButtons.count()) === 0;
        },
        "applied card actions to clear"
      );

      const horizontalOverflow = await page.evaluate(
        () => document.documentElement.scrollWidth > window.innerWidth + 4
      );
      const screenshotPath = path.join(outDir, "harness-review-flow.png");
      await page.screenshot({ path: screenshotPath, fullPage: true });

      const contract = JSON.parse(await readFile(seeded.contractPath, "utf8"));
      const gate = JSON.parse(await readFile(seeded.gatePath, "utf8"));
      const appliedTemplate =
        contract
        && contract.runtime_overlay_templates
        && contract.runtime_overlay_templates[REVIEW_ID];
      const reviewRecord =
        gate
        && gate.reviews
        && gate.reviews[REVIEW_ID];
      const jumpCountAfterApply = await page
        .getByRole("button", { name: /Harness reviews/i })
        .count();

      const checks = {
        jumpButtonVisible,
        panelScrollsIntoView: await isSelectorInViewport(page, ".panel-review"),
        finalAppliedStatus: await locatorContainsAny(reviewCard, REVIEW_TEXT.applied),
        contractUpdated:
          !!appliedTemplate
          && appliedTemplate.policy_action === "advance_repo_scaffold_artifact",
        gateUpdated:
          !!reviewRecord
          && String(reviewRecord.decision || "") === "applied",
        jumpClearedAfterApply: jumpCountAfterApply === 0,
        horizontalOverflow: !horizontalOverflow,
      };

      return {
        kind: "harness_review_flow",
        seededWorkspaceRoot: seeded.root,
        screenshotPath,
        checks,
        candidatePath: seeded.candidatePath,
        gatePath: seeded.gatePath,
        contractPath: seeded.contractPath,
        ok: Object.values(checks).every(Boolean),
      };
    }
  );
}

async function runRuntimeApprovalFlowScenario(args, outDir) {
  const seeded = await seedRuntimeApprovalWorkspace(outDir);
  return runServerScenario(
    "runtime-approval-flow",
    seeded.root,
    args.host,
    args.port,
    outDir,
    async ({ page, baseUrl }) => {
      await postJson(new URL("api/queue_edit", baseUrl), {
        action: "write_file",
        path: APPROVAL_PATHS.edit,
        content: "Runtime approval edit applied by Playwright smoke.\n",
      });
      await postJson(new URL("api/queue_command", baseUrl), {
        command: `printf 'approved via runtime approval smoke\\n' > ${APPROVAL_PATHS.commandApproved}`,
      });
      await postJson(new URL("api/queue_command", baseUrl), {
        command: `printf 'this file should not exist\\n' > ${APPROVAL_PATHS.commandRejected}`,
      });

      await page.setViewportSize({ width: 1440, height: 1800 });
      await page.goto(baseUrl, { waitUntil: "domcontentloaded", timeout: 30000 });
      await page.waitForSelector(".panel", { timeout: 15000 });
      await page.waitForTimeout(800);
      await ensureEnglish(page);

      const runtimeJump = page.getByRole("button", { name: /Runtime approvals/i }).first();
      await runtimeJump.waitFor({ timeout: 15000 });
      const jumpButtonVisible = await runtimeJump.isVisible();
      await runtimeJump.click();
      await waitForCondition(
        () => isSelectorInViewport(page, ".approval-board"),
        "runtime approvals panel to scroll into view"
      );

      const runtimePanel = page.locator(".panel").filter({ hasText: "Runtime approvals" }).first();
      const editCard = runtimePanel
        .locator(".approval-card")
        .filter({ hasText: APPROVAL_PATHS.edit })
        .first();
      const approvedCommandCard = runtimePanel
        .locator(".approval-card")
        .filter({ hasText: APPROVAL_PATHS.commandApproved })
        .first();
      const rejectedCommandCard = runtimePanel
        .locator(".approval-card")
        .filter({ hasText: APPROVAL_PATHS.commandRejected })
        .first();

      await editCard.waitFor({ timeout: 15000 });
      await approvedCommandCard.waitFor({ timeout: 15000 });
      await rejectedCommandCard.waitFor({ timeout: 15000 });

      await editCard.getByRole("button", { name: /^(Approve|承認|Approuver)$/ }).click();
      await waitForCondition(
        async () => (await editCard.innerText()).includes("approved"),
        "approved edit card"
      );
      await waitForCondition(
        async () => (await editCard.locator("button").count()) === 0,
        "approved edit actions to clear"
      );

      await approvedCommandCard.getByRole("button", { name: /^(Approve|承認|Approuver)$/ }).click();
      await waitForCondition(
        async () => (await approvedCommandCard.innerText()).includes("approved"),
        "approved command card"
      );
      await waitForCondition(
        async () => (await approvedCommandCard.locator("button").count()) === 0,
        "approved command actions to clear"
      );

      await rejectedCommandCard.getByRole("button", { name: /^(Reject|却下|Rejeter)$/ }).click();
      await waitForCondition(
        async () => (await rejectedCommandCard.innerText()).includes("rejected"),
        "rejected command card"
      );
      await waitForCondition(
        async () => (await rejectedCommandCard.locator("button").count()) === 0,
        "rejected command actions to clear"
      );

      await waitForCondition(
        async () => (await page.getByRole("button", { name: /Runtime approvals/i }).count()) === 0,
        "runtime approvals jump button to clear"
      );

      const horizontalOverflow = await page.evaluate(
        () => document.documentElement.scrollWidth > window.innerWidth + 4
      );
      const screenshotPath = path.join(outDir, "runtime-approval-flow.png");
      await page.screenshot({ path: screenshotPath, fullPage: true });

      const editContent = await readFile(seeded.editPath, "utf8").catch(() => "");
      const approvedCommandExists = !!(await readFile(seeded.approvedCommandPath, "utf8").catch(() => ""));
      const rejectedCommandExists = !!(await readFile(seeded.rejectedCommandPath, "utf8").catch(() => ""));

      const checks = {
        jumpButtonVisible,
        panelScrollsIntoView: await isSelectorInViewport(page, ".approval-board"),
        editApproved: (await editCard.innerText()).includes("approved"),
        approvedCommandExecuted: approvedCommandExists,
        rejectedCommandStayedBlocked: !rejectedCommandExists,
        editFileWritten: editContent.includes("Runtime approval edit applied by Playwright smoke."),
        jumpClearedAfterResolve: (await page.getByRole("button", { name: /Runtime approvals/i }).count()) === 0,
        horizontalOverflow: !horizontalOverflow,
      };

      return {
        kind: "runtime_approval_flow",
        seededWorkspaceRoot: seeded.root,
        screenshotPath,
        checks,
        editPath: seeded.editPath,
        approvedCommandPath: seeded.approvedCommandPath,
        rejectedCommandPath: seeded.rejectedCommandPath,
        ok: Object.values(checks).every(Boolean),
      };
    }
  );
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const outDir = path.join(repoRoot, ".tmp", `ui_smoke_${nowStamp()}`);
  await mkdir(outDir, { recursive: true });
  const reportPath = path.join(outDir, "report.json");
  try {
    const requestedScenarios =
      args.scenario === "all"
        ? ["baseline", "harness-review-flow", "runtime-approval-flow"]
        : [args.scenario];

    const scenarios = [];
    if (requestedScenarios.includes("baseline")) {
      scenarios.push(await runBaselineScenario(args, outDir));
    }
    if (requestedScenarios.includes("harness-review-flow")) {
      scenarios.push(await runHarnessReviewFlowScenario(args, outDir));
    }
    if (requestedScenarios.includes("runtime-approval-flow")) {
      scenarios.push(await runRuntimeApprovalFlowScenario(args, outDir));
    }
    if (!scenarios.length) {
      throw new Error(
        `unsupported scenario '${args.scenario}'. Use baseline, harness-review-flow, runtime-approval-flow, or all.`
      );
    }

    const baseline = scenarios.find((scenario) => scenario.name === "baseline");
    const report = {
      ok: scenarios.every((scenario) => scenario.ok),
      workspaceRoot: args.workspaceRoot,
      url: `http://${args.host}:${args.port}/`,
      scenarios,
      results: baseline && Array.isArray(baseline.results) ? baseline.results : [],
      pageErrors: scenarios.flatMap((scenario) => scenario.pageErrors || []),
      consoleErrors: scenarios.flatMap((scenario) => scenario.consoleErrors || []),
      badResponses: scenarios.flatMap((scenario) => scenario.badResponses || []),
    };

    await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
    console.log(`[ui-smoke] report=${reportPath}`);
    scenarios.forEach((scenario) => {
      console.log(`[ui-smoke] scenario=${scenario.name} ok=${scenario.ok}`);
      if (Array.isArray(scenario.results)) {
        scenario.results.forEach((result) => {
          console.log(
            `[ui-smoke] ${result.name} checks=${JSON.stringify(result.checks)} screenshot=${result.screenshotPath}`
          );
        });
      }
      if (scenario.screenshotPath) {
        console.log(`[ui-smoke] ${scenario.name} screenshot=${scenario.screenshotPath}`);
      }
    });
    if (!report.ok) {
      process.exitCode = 1;
    }
  } catch (error) {
    const failureReport = {
      ok: false,
      workspaceRoot: args.workspaceRoot,
      url: `http://${args.host}:${args.port}/`,
      error: String(error && error.stack ? error.stack : error),
    };
    await writeFile(reportPath, `${JSON.stringify(failureReport, null, 2)}\n`, "utf8");
    console.log(`[ui-smoke] report=${reportPath}`);
    throw error;
  }
}

try {
  await main();
} catch (error) {
  console.error(error);
  process.exitCode = 1;
}
