[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [ValidateSet("atlas-drilldown", "api-flow", "change-impact", "source-jump", "large-repo")]
  [string]$Scenario,
  [int]$Port = 9222,
  [ValidateRange(800, 3840)]
  [int]$Width = 1440,
  [ValidateRange(600, 2160)]
  [int]$Height = 900,
  [switch]$ExerciseReveal,
  [string]$Screenshot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$helper = Join-Path $PSScriptRoot "tauri-cdp-smoke.mjs"
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$expressionPath = Join-Path $tempBase ("backend-visual-map-ui-smoke-" + [guid]::NewGuid().ToString("N") + ".js")

$atlasExpression = @'
(async () => {
  const waitFor = async (selector, timeout = 8000) => {
    const started = Date.now();
    while (Date.now() - started < timeout) {
      const node = document.querySelector(selector);
      if (node) return node;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(`Timed out waiting for ${selector}`);
  };

  const atlasView = document.querySelector('button[data-view="atlas"]');
  if (atlasView) atlasView.click();
  const overview = await waitFor('button[data-mode-id="atlas"]');
  overview.click();
  const card = await waitFor('.at-domain-card');
  card.click();
  await waitFor('.at-domain-band');
  const bands = [...document.querySelectorAll('.at-domain-band')];
  if (bands.length !== 3) throw new Error(`Expected 3 detail bands, got ${bands.length}`);
  const labels = bands.map((band) => band.getAttribute('data-domain-band') ?? '');
  if (labels.join('|') !== '1|2|3') throw new Error(`Unexpected band order: ${labels.join('|')}`);
  if (document.documentElement.scrollWidth > window.innerWidth + 2) throw new Error('Root document overflows horizontally');
  return { ok: true, card: card.textContent?.trim(), labels };
})()
'@

$impactExpression = @'
(async () => {
  const waitFor = async (selector, timeout = 8000) => {
    const started = Date.now();
    while (Date.now() - started < timeout) {
      const node = document.querySelector(selector);
      if (node) return node;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(`Timed out waiting for ${selector}`);
  };

  document.querySelector('button[data-view="atlas"]')?.click();
  await waitFor('.mode-card');
  const mode = document.querySelector('button[data-mode-id="impact"]');
  if (!mode || mode.disabled) throw new Error('Column impact mode is not available for the loaded workspace');
  mode.click();
  let board = null;
  const started = Date.now();
  while (Date.now() - started < 8000) {
    const candidate = document.querySelector('.at-impact-board:not(.at-api-reading)');
    if (candidate?.querySelectorAll('.at-impact-lanes > .at-impact-lane').length === 4) {
      board = candidate;
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  if (!board) throw new Error('Timed out waiting for the column impact review board');
  const labels = [...board.querySelectorAll('.at-impact-lanes > .at-impact-lane')]
    .map((node) => node.getAttribute('aria-labelledby') ?? '');
  const expected = ['impact-lane-direct', 'impact-lane-candidates', 'impact-lane-unknowns', 'impact-lane-checks'];
  if (labels.join('|') !== expected.join('|')) throw new Error(`Unexpected impact lanes: ${labels.join('|')}`);
  const surface = document.querySelector('.at-map-surface');
  const surfaceRect = surface?.getBoundingClientRect();
  const clipped = surfaceRect && [...board.querySelectorAll('.at-impact-lanes > .at-impact-lane')]
    .some((lane) => {
      const rect = lane.getBoundingClientRect();
      return rect.left < surfaceRect.left - 2 || rect.right > surfaceRect.right + 2;
    });
  if (clipped) throw new Error('Impact lane is horizontally clipped');
  if (document.documentElement.scrollWidth > window.innerWidth + 2) throw new Error('Root document overflows horizontally');
  return { ok: true, labels };
})()
'@

$apiExpression = @'
(async () => {
  const waitFor = async (selector, timeout = 8000) => {
    const started = Date.now();
    while (Date.now() - started < timeout) {
      const node = document.querySelector(selector);
      if (node) return node;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(`Timed out waiting for ${selector}`);
  };

  document.querySelector('button[data-view="atlas"]')?.click();
  await waitFor('.mode-card');
  const mode = document.querySelector('button[data-mode-id="api"]');
  if (!mode || mode.disabled) throw new Error('API flow mode is not available for the loaded workspace');
  mode.click();
  await waitFor('.at-api-reading');
  const labels = [...document.querySelectorAll('.at-api-reading > .at-impact-lanes > .at-impact-lane')]
    .map((node) => node.getAttribute('aria-labelledby') ?? '');
  const expected = [
    'api-lane-route',
    'api-lane-handler',
    'api-lane-service-function',
    'api-lane-repository-query',
    'api-lane-db-candidate',
  ];
  if (labels.join('|') !== expected.join('|')) throw new Error(`Unexpected API lanes: ${labels.join('|')}`);
  const routeItems = document.querySelectorAll('[aria-labelledby="api-lane-route"] .at-impact-item').length;
  if (routeItems !== 1) throw new Error(`Expected one selected route, got ${routeItems}`);
  if (document.querySelectorAll('.at-api-followups > .at-impact-lane').length !== 2) {
    throw new Error('Expected unknown and recommended-check follow-up lanes');
  }
  const surface = document.querySelector('.at-map-surface');
  const surfaceRect = surface?.getBoundingClientRect();
  const clipped = surfaceRect && [...document.querySelectorAll('.at-api-reading > .at-impact-lanes > .at-impact-lane')]
    .some((lane) => {
      const rect = lane.getBoundingClientRect();
      return rect.left < surfaceRect.left - 2 || rect.right > surfaceRect.right + 2;
    });
  if (clipped) throw new Error('API lane is horizontally clipped');
  if (document.documentElement.scrollWidth > window.innerWidth + 2) throw new Error('Root document overflows horizontally');
  return { ok: true, labels };
})()
'@

$sourceJumpExpression = @'
(async () => {
  const exerciseReveal = __EXERCISE_REVEAL__;
  const waitFor = async (selector, timeout = 8000) => {
    const started = Date.now();
    while (Date.now() - started < timeout) {
      const node = document.querySelector(selector);
      if (node) return node;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(`Timed out waiting for ${selector}`);
  };

  const atlasView = document.querySelector('button[data-view="atlas"]');
  if (atlasView) atlasView.click();
  const overview = await waitFor('button[data-mode-id="atlas"]');
  overview.click();
  const card = await waitFor('.at-domain-card');
  card.click();
  const sourceNode = await waitFor('.at-domain-band[data-domain-band="2"] .at-domain-member');
  sourceNode.click();
  await waitFor('[data-source-action="vscode"]');
  const actions = [...document.querySelectorAll('[data-source-action]')]
    .map((node) => node.getAttribute('data-source-action') || '');
  const expected = ['vscode', 'cursor', 'reveal'];
  if (actions.join('|') !== expected.join('|')) throw new Error(`Unexpected source actions: ${actions.join('|')}`);
  const add = await waitFor('[data-investigation-action="add"]');
  if (!add.disabled) add.click();
  const tray = await waitFor('.investigation-tray');
  const item = await waitFor('.investigation-item');
  const toggle = await waitFor('[data-investigation-action="toggle"]');
  const checkedBefore = toggle.getAttribute('aria-pressed');
  toggle.click();
  const toggleStarted = Date.now();
  while (toggle.getAttribute('aria-pressed') === checkedBefore && Date.now() - toggleStarted < 2000) {
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  if (toggle.getAttribute('aria-pressed') === checkedBefore) throw new Error('Investigation checked state did not update');
  if (!tray.querySelector('[data-investigation-storage="path-evidence-id-only"]')) throw new Error('Investigation privacy boundary is missing');
  const evidence = item.querySelector('code');
  if (!evidence || !evidence.textContent || !evidence.textContent.trim()) throw new Error('Investigation evidence ID is missing');
  const storageKey = Object.keys(localStorage).find((key) => key.startsWith('backend-visual-map:investigation:v1:'));
  const stored = storageKey ? JSON.parse(localStorage.getItem(storageKey) || '[]') : [];
  const allowedKeys = ['checked', 'column', 'evidenceId', 'line', 'path'];
  if (!stored.length || Object.keys(stored[0]).sort().join('|') !== allowedKeys.join('|')) {
    throw new Error('Investigation storage contains missing or unexpected fields');
  }
  const copy = await waitFor('[data-investigation-action="copy"]');
  copy.click();
  const copyStarted = Date.now();
  while (copy.getAttribute('data-copy-state') === 'idle' && Date.now() - copyStarted < 2000) {
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  if (copy.getAttribute('data-copy-state') === 'idle') throw new Error('Investigation Markdown export did not finish');
  if (exerciseReveal) {
    document.querySelector('[data-source-action="reveal"]').click();
    await waitFor('[data-source-status="success"]');
  }
  tray.scrollIntoView({ block: 'center' });
  return { ok: true, labels: [...actions, 'investigation'] };
})()
'@
$sourceJumpExpression = $sourceJumpExpression.Replace(
  "__EXERCISE_REVEAL__",
  $ExerciseReveal.IsPresent.ToString().ToLowerInvariant()
)

$largeRepoExpression = @'
(async () => {
  const waitFor = async (selector, timeout = 8000) => {
    const started = Date.now();
    while (Date.now() - started < timeout) {
      const node = document.querySelector(selector);
      if (node) return node;
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    throw new Error(`Timed out waiting for ${selector}`);
  };

  const atlasView = document.querySelector('button[data-view="atlas"]');
  if (atlasView) atlasView.click();
  const overview = await waitFor('button[data-mode-id="atlas"]');
  overview.click();
  await waitFor('.at-domain-card');
  const cards = document.querySelectorAll('.at-domain-card').length;
  if (cards > 40) throw new Error(`Overview exceeds 40 cards: ${cards}`);

  const input = await waitFor('#global-inventory-search');
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
  setter.call(input, '');
  input.dispatchEvent(new Event('input', { bubbles: true }));
  const clearStarted = Date.now();
  while (document.querySelector('.search-popover .search-result') && Date.now() - clearStarted < 1000) {
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  const searchStarted = performance.now();
  setter.call(input, 'audio');
  input.dispatchEvent(new Event('input', { bubbles: true }));
  await waitFor('.search-popover .search-result', 300);
  const searchMs = Math.round(performance.now() - searchStarted);
  if (searchMs > 300) throw new Error(`Search exceeded 300ms: ${searchMs}ms`);
  let diagnosticsText = null;
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: async (value) => { diagnosticsText = value; } },
  });
  const diagnostics = await waitFor('[data-diagnostics-action="copy"]');
  diagnostics.click();
  const diagnosticsStarted = Date.now();
  while (!diagnosticsText && Date.now() - diagnosticsStarted < 2000) {
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  if (!diagnosticsText) throw new Error('Diagnostics export did not reach the clipboard boundary');
  const bundle = JSON.parse(diagnosticsText);
  const serialized = JSON.stringify(bundle);
  const forbiddenKeys = ['workspaceId', 'workspaceName', 'repoPath', 'path', 'executable', 'engineDir', 'details', 'error'];
  if (forbiddenKeys.some((key) => serialized.includes(`"${key}"`))) {
    throw new Error('Diagnostics export contains a forbidden identifying or error field');
  }
  if (bundle.schemaVersion !== 1 || typeof bundle.projection.elapsedMs !== 'number') {
    throw new Error('Diagnostics export is missing its schema or projection timing');
  }
  if (document.documentElement.scrollWidth > window.innerWidth + 2) throw new Error('Root document overflows horizontally');
  return { ok: true, labels: [`cards:${cards}`, `search:${searchMs}ms`, 'diagnostics:redacted', `viewport:${window.innerWidth}x${window.innerHeight}`] };
})()
'@

$expression = if ($Scenario -eq "change-impact") {
  $impactExpression
}
elseif ($Scenario -eq "api-flow") {
  $apiExpression
}
elseif ($Scenario -eq "source-jump") {
  $sourceJumpExpression
}
elseif ($Scenario -eq "large-repo") {
  $largeRepoExpression
}
else {
  $atlasExpression
}

try {
  [IO.File]::WriteAllText($expressionPath, $expression, [Text.UTF8Encoding]::new($false))
  $arguments = @(
    $helper,
    "--port", [string]$Port,
    "--width", [string]$Width,
    "--height", [string]$Height,
    "--eval-file", $expressionPath
  )
  if (-not [string]::IsNullOrWhiteSpace($Screenshot)) {
    $arguments += @("--screenshot", [IO.Path]::GetFullPath($Screenshot))
  }
  $output = & node @arguments
  if ($LASTEXITCODE -ne 0) {
    throw "UI smoke failed with exit code $LASTEXITCODE"
  }
  $result = $output | ConvertFrom-Json
  if ($result.value.ok -ne $true) {
    throw "UI smoke did not return a passing result"
  }
  Write-Output "PASS ${Scenario}: $($result.value.labels -join ' -> ')"
}
finally {
  $resolved = [IO.Path]::GetFullPath($expressionPath)
  if ($resolved.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) {
    Remove-Item -LiteralPath $resolved -Force -ErrorAction SilentlyContinue
  }
}
