[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [ValidateSet("atlas-drilldown", "api-flow", "change-impact", "source-jump", "large-repo", "stable-navigation", "semantic-composition")]
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

  const overview = await waitFor('button[data-mode-id="atlas"]');
  overview.click();
  await new Promise((resolve) => setTimeout(resolve, 50));
  const resetStarted = Date.now();
  while (!document.querySelector('.at-domain-card') && Date.now() - resetStarted < 8000) {
    const back = document.querySelector('button[data-atlas-action="overview"]');
    if (back && !back.disabled) {
      back.click();
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
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

  await waitFor('.product-nav-list');
  const overview = await waitFor('button[data-mode-id="atlas"]');
  overview.click();
  await new Promise((resolve) => setTimeout(resolve, 50));
  const resetStarted = Date.now();
  while (!document.querySelector('.at-domain-card') && Date.now() - resetStarted < 8000) {
    const back = document.querySelector('button[data-atlas-action="overview"]');
    if (back && !back.disabled) {
      back.click();
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  await waitFor('.at-domain-card');
  const mode = document.querySelector('button[data-mode-id="impact"]');
  if (!mode || mode.disabled) throw new Error('Column impact mode is not available for the loaded workspace');
  mode.click();
  await new Promise((resolve) => setTimeout(resolve, 0));
  await waitFor('button[data-mode-id="impact"].active');
  const transitionStarted = Date.now();
  while (document.querySelector('.canvas[aria-busy="true"]') && Date.now() - transitionStarted < 8000) {
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  if (document.querySelector('.canvas[aria-busy="true"]')) throw new Error('Column impact mode did not settle');
  let board = document.querySelector('.at-impact-board:not(.at-api-reading)');
  if (!board) {
    const columnTarget = document.querySelector('.product-context-list button[data-context-id]');
    if (!columnTarget) throw new Error('Column impact mode has no selectable column in the left context list');
    columnTarget.click();
  }
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
  const enrichmentStarted = Date.now();
  while (document.querySelector('.status-quality')?.getAttribute('data-enriching') === 'true' && Date.now() - enrichmentStarted < 8000) {
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  if (document.querySelector('.status-quality')?.getAttribute('data-enriching') === 'true') {
    throw new Error('Code evidence enrichment did not settle');
  }
  mode.click();
  await new Promise((resolve) => setTimeout(resolve, 80));
  if (document.querySelector('.is-transitioning')) throw new Error('Clicking the active impact mode restarted loading');
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

  await waitFor('.product-nav-list');
  const overview = await waitFor('button[data-mode-id="atlas"]');
  overview.click();
  await new Promise((resolve) => setTimeout(resolve, 50));
  const resetStarted = Date.now();
  while (!document.querySelector('.at-domain-card') && Date.now() - resetStarted < 8000) {
    const back = document.querySelector('button[data-atlas-action="overview"]');
    if (back && !back.disabled) {
      back.click();
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  await waitFor('.at-domain-card');
  const workspace = await waitFor('.product-workspace');
  const columnsBefore = getComputedStyle(workspace).gridTemplateColumns;
  if (!document.querySelector('.evidence-panel')) throw new Error('Desktop evidence panel is not reserved');
  const mode = document.querySelector('button[data-mode-id="api"]');
  if (!mode || mode.disabled) throw new Error('API flow mode is not available for the loaded workspace');
  mode.click();
  await new Promise((resolve) => setTimeout(resolve, 0));
  if (document.querySelector('.canvas[aria-busy="true"]')) {
    if (!document.querySelector('.at-canvas.is-refreshing')) throw new Error('API loading did not retain the committed canvas');
    if (!document.querySelector('.at-map-surface')) throw new Error('API loading removed the committed canvas');
    if (!document.querySelector('button[data-mode-id="api"][aria-busy="true"]')) throw new Error('API mode did not expose its pending state');
    if (document.querySelector('button[data-mode-id="api"].active')) throw new Error('API mode became active before its projection committed');
  }
  await waitFor('.api-connection-view');
  const nodes = [...document.querySelectorAll('.api-diagram-node[data-node-id]')];
  if (nodes.length === 0) throw new Error('API connection map has no real nodes');
  const nodeIds = nodes.map((node) => node.getAttribute('data-node-id') || '');
  if (new Set(nodeIds).size !== nodeIds.length) throw new Error('API connection map rendered duplicate primary nodes');
  if (!document.querySelector('.product-context-list button.active')) throw new Error('Selected API route is not reflected in the left navigation');
  const columnsAfter = getComputedStyle(workspace).gridTemplateColumns;
  if (columnsAfter !== columnsBefore) throw new Error(`Workspace columns shifted: ${columnsBefore} -> ${columnsAfter}`);
  nodes[0].click();
  const inspector = await waitFor('.inspector');
  const inspectorSections = [...inspector.querySelectorAll('.inspector-section > header > strong')]
    .map((node) => node.textContent?.trim() ?? '');
  if (inspectorSections.length !== 5) {
    throw new Error(`Unexpected inspector order: ${inspectorSections.join('|')}`);
  }
  const layers = document.querySelector('button[data-api-view="layers"]');
  layers.click();
  await waitFor('.api-layer-view');
  if (document.querySelectorAll('.api-layer-view > section').length !== 5) throw new Error('API hierarchy view is incomplete');
  const list = document.querySelector('button[data-api-view="list"]');
  list.click();
  await waitFor('.api-list-view');
  if (document.querySelectorAll('.api-list-view > div').length === 0) throw new Error('API list view is empty');
  document.querySelector('button[data-api-view="connections"]').click();
  await waitFor('.api-connection-view');
  const branchToggle = document.querySelector('.api-branch-toggle');
  if (branchToggle) {
    branchToggle.click();
    await waitFor('.api-branch-drawer');
    branchToggle.click();
    const branchClosedStarted = Date.now();
    while (document.querySelector('.api-branch-drawer') && Date.now() - branchClosedStarted < 2000) {
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    if (document.querySelector('.api-branch-drawer')) throw new Error('Additional API connections did not collapse');
  }
  mode.click();
  await new Promise((resolve) => setTimeout(resolve, 80));
  if (document.querySelector('.is-transitioning')) throw new Error('Clicking the active API mode restarted loading');
  if (document.documentElement.scrollWidth > window.innerWidth + 2) throw new Error('Root document overflows horizontally');
  return { ok: true, labels: [`nodes:${nodes.length}`, 'views:3', branchToggle ? 'branches:expanded' : 'branches:none'] };
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

  const overview = await waitFor('button[data-mode-id="atlas"]');
  overview.click();
  await new Promise((resolve) => setTimeout(resolve, 50));
  const resetStarted = Date.now();
  while (!document.querySelector('.at-domain-card') && Date.now() - resetStarted < 8000) {
    const back = document.querySelector('button[data-atlas-action="overview"]');
    if (back && !back.disabled) {
      back.click();
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  const card = await waitFor('.at-domain-card.code');
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
  if (!tray.querySelector('[data-investigation-storage="source-location-evidence-check-state-only"]')) throw new Error('Investigation privacy boundary is missing');
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

  const overview = await waitFor('button[data-mode-id="atlas"]');
  overview.click();
  await new Promise((resolve) => setTimeout(resolve, 50));
  const resetStarted = Date.now();
  while (!document.querySelector('.at-domain-card') && Date.now() - resetStarted < 8000) {
    const back = document.querySelector('button[data-atlas-action="overview"]');
    if (back && !back.disabled) {
      back.click();
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
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
  const originalExecCommand = document.execCommand;
  document.execCommand = (command) => {
    if (command === 'copy' && document.activeElement instanceof HTMLTextAreaElement) {
      diagnosticsText = document.activeElement.value;
      return true;
    }
    return originalExecCommand.call(document, command);
  };
  const diagnostics = await waitFor('[data-diagnostics-action="copy"]');
  diagnostics.click();
  const diagnosticsStarted = Date.now();
  while (!diagnosticsText && Date.now() - diagnosticsStarted < 2000) {
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  if (!diagnosticsText) throw new Error('Diagnostics export did not reach the clipboard boundary');
  document.execCommand = originalExecCommand;
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

$stableNavigationExpression = @'
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
  const waitForSettled = async () => {
    await new Promise((resolve) => setTimeout(resolve, 25));
    const started = Date.now();
    while (document.querySelector('.canvas[aria-busy="true"]') && Date.now() - started < 8000) {
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    if (document.querySelector('.canvas[aria-busy="true"]')) throw new Error('Mode transition did not settle');
  };

  const workspace = await waitFor('.product-workspace');
  const initialOverview = await waitFor('button[data-mode-id="atlas"]');
  initialOverview.click();
  await waitForSettled();
  await waitFor('.at-map-surface');
  const modes = [
    ['atlas', '개요'],
    ['api', 'API'],
    ['search', '코드'],
    ['dependencies', '데이터베이스'],
    ['impact', '변경 영향'],
  ];
  const columns = getComputedStyle(workspace).gridTemplateColumns;
  let headerHeight = null;

  for (const [id, label] of modes) {
    const button = await waitFor(`button[data-mode-id="${id}"]`);
    if (button.disabled) throw new Error(`${label} mode is unavailable`);
    button.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    if (document.querySelector('.canvas[aria-busy="true"]')) {
      if (!document.querySelector('.at-canvas.is-refreshing')) throw new Error(`${label} loading did not retain the committed canvas`);
      if (!document.querySelector('.at-map-surface') && !document.querySelector('.at-stage > .map-empty')) {
        throw new Error(`${label} removed the committed answer while loading`);
      }
      if (!document.querySelector(`button[data-mode-id="${id}"][aria-busy="true"]`)) throw new Error(`${label} did not expose its pending state`);
      if (document.querySelector(`button[data-mode-id="${id}"].active`)) throw new Error(`${label} became active before its projection committed`);
    }
    await waitForSettled();
    const active = document.querySelector(`button[data-mode-id="${id}"].active`);
    if (!active) throw new Error(`${label} did not become active`);
    if (id !== 'atlas' && !document.querySelector('.product-context-list button.active') && !document.querySelector('.target-selection-empty')) {
      throw new Error(`${label} exposed neither a current target nor the neutral target prompt`);
    }
    const nextColumns = getComputedStyle(workspace).gridTemplateColumns;
    if (nextColumns !== columns) throw new Error(`${label} shifted workspace columns: ${columns} -> ${nextColumns}`);
    const nextHeaderHeight = Math.round(document.querySelector('.at-canvas-head').getBoundingClientRect().height);
    if (headerHeight === null) headerHeight = nextHeaderHeight;
    if (nextHeaderHeight !== headerHeight) throw new Error(`${label} shifted header height: ${headerHeight} -> ${nextHeaderHeight}`);
  }

  if (window.innerWidth <= 1020) {
    const contextToggle = document.querySelector('.product-context-toggle');
    if (!contextToggle) throw new Error('Compact navigation has no target-list control');
    contextToggle.click();
    const compactContext = await waitFor('.product-context-browser.compact-open');
    if (!compactContext.querySelector('.product-context-list') || !compactContext.querySelector('.product-context-filter')) {
      throw new Error('Compact context panel is missing its fixed filter or target list');
    }
    compactContext.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 25));
    if (document.querySelector('.product-context-browser.compact-open')) {
      throw new Error('Compact context panel did not close with Escape');
    }
    if (document.activeElement !== contextToggle) throw new Error('Compact target-list control did not regain focus after Escape');
    contextToggle.click();
    const reopenedContext = await waitFor('.product-context-browser.compact-open');
    const closeContext = reopenedContext.querySelector('.product-context-close');
    if (!closeContext) throw new Error('Compact context panel has no close control');
    closeContext.click();
    await new Promise((resolve) => setTimeout(resolve, 25));
    if (document.querySelector('.product-context-browser.compact-open')) {
      throw new Error('Compact context panel did not close');
    }
  }

  document.querySelector('button[data-mode-id="api"]').click();
  await waitForSettled();
  const apiItems = [...document.querySelectorAll('.product-context-list button[data-context-id]')];
  if (apiItems.length < 2) throw new Error('API context list does not expose enough routes to test restoration');
  apiItems[1].click();
  await waitForSettled();
  const restoredRoute = document.querySelector('.product-context-list button.active')?.getAttribute('data-context-id');
  if (!restoredRoute) throw new Error('API target did not become active');
  const zoomInIcon = await waitFor('.api-map-floating-controls .lucide-plus');
  const zoomIn = zoomInIcon.closest('button');
  if (!zoomIn) throw new Error('API zoom control is missing');
  const zoomBefore = document.querySelector('.api-map-floating-controls .wide')?.textContent?.trim();
  zoomIn.click();
  const zoomStarted = Date.now();
  while (document.querySelector('.api-map-floating-controls .wide')?.textContent?.trim() === zoomBefore && Date.now() - zoomStarted < 1000) {
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  const restoredZoom = document.querySelector('.api-map-floating-controls .wide')?.textContent?.trim();
  document.querySelector('button[data-mode-id="search"]').click();
  await waitForSettled();
  document.querySelector('button[data-mode-id="api"]').click();
  await waitForSettled();
  if (document.querySelector('.product-context-list button.active')?.getAttribute('data-context-id') !== restoredRoute) {
    throw new Error('API mode did not restore its last target');
  }
  if (document.querySelector('.api-map-floating-controls .wide')?.textContent?.trim() !== restoredZoom) {
    throw new Error('API mode did not restore its zoom');
  }

  document.querySelector('button[data-mode-id="search"]').click();
  await waitForSettled();
  let codeCard = document.querySelector('.at-card.code');
  let disconnectedTarget = document.querySelector('.at-disconnected-target');
  if (!codeCard && !disconnectedTarget) {
    const codeTarget = document.querySelector('.product-context-list button[data-context-id]');
    if (!codeTarget) throw new Error('Code mode has no target in the left context list');
    codeTarget.click();
    await waitForSettled();
    codeCard = document.querySelector('.at-card.code');
    disconnectedTarget = document.querySelector('.at-disconnected-target');
  }
  if (!codeCard && !disconnectedTarget) throw new Error('Code mode has no focused target');
  if (codeCard) codeCard.click();
  await new Promise((resolve) => setTimeout(resolve, 25));
  const selectionStarted = Date.now();
  while (!workspace.classList.contains('inspector-visible') && Date.now() - selectionStarted < 1000) {
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  if (document.querySelector('.at-update-indicator')) throw new Error('Selecting a code card restarted projection');
  if (!document.querySelector('button[data-mode-id="search"].active')) throw new Error('Selecting a code card changed mode');
  if (!workspace.classList.contains('inspector-visible')) throw new Error('Selecting a code card did not update evidence');
  const inspectorBody = document.querySelector('.inspector-scroll-body');
  const sourceAction = document.querySelector('button[data-source-action="reveal"]');
  const nextCheck = document.querySelector('.inspector > .inspector-section:last-child');
  if (!inspectorBody || !sourceAction || !nextCheck) throw new Error('Inspector scroll/footer structure is incomplete');
  sourceAction.scrollIntoView({ block: 'end' });
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  const sourceRect = sourceAction.getBoundingClientRect();
  const bodyRect = inspectorBody.getBoundingClientRect();
  const nextRect = nextCheck.getBoundingClientRect();
  if (sourceRect.top < bodyRect.top - 1 || sourceRect.bottom > bodyRect.bottom + 1 || sourceRect.bottom > nextRect.top) {
    throw new Error('Fixed next-check footer covers source actions');
  }
  document.querySelector('button[data-mode-id="api"]').click();
  await waitForSettled();

  const clearSelection = document.querySelector('.inspector-close');
  if (!clearSelection) throw new Error('Selection clear control is missing');
  clearSelection.click();
  await new Promise((resolve) => setTimeout(resolve, 25));
  const evidencePanel = document.querySelector('.evidence-panel');
  if (!evidencePanel) throw new Error('Evidence panel was removed after clearing selection');
  if (window.innerWidth > 820 && getComputedStyle(evidencePanel).display === 'none') {
    throw new Error('Desktop evidence panel collapsed after clearing selection');
  }
  if (window.innerWidth <= 820 && getComputedStyle(evidencePanel).display !== 'none') {
    throw new Error('Compact evidence overlay stayed open after clearing selection');
  }

  const sourceTrigger = await waitFor('.source-manager-trigger');
  if (!document.querySelector('.source-manager')) sourceTrigger.click();
  const sourceManager = await waitFor('.source-manager');
  const dbDetails = sourceManager.querySelector('.database-source details.source-advanced');
  if (dbDetails) dbDetails.open = true;
  const sourceActions = [...sourceManager.querySelectorAll('.database-source [data-source-action]')]
    .map((button) => button.getAttribute('data-source-action'));
  const expectedSourceActions = ['db-index'];
  if (sourceActions.join('|') !== expectedSourceActions.join('|')) {
    throw new Error(`Unexpected DB source actions: ${sourceActions.join('|')}`);
  }
  sourceManager.querySelector('.source-manager-header .tool')?.click();

  if (document.documentElement.scrollWidth > window.innerWidth + 2) throw new Error('Root document overflows horizontally');
  return { ok: true, labels: [`modes:${modes.length}`, `header:${headerHeight}px`, `target:restored`, `zoom:${restoredZoom}`, `source-actions:${sourceActions.length}`] };
})()
'@

$compositionExpression = @'
(async () => {
  const waitFor = async (selector, timeout = 10000) => {
    const started = Date.now();
    while (Date.now() - started < timeout) {
      const node = document.querySelector(selector);
      if (node) return node;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(`Timed out waiting for ${selector}`);
  };
  const waitForSettled = async () => {
    const started = Date.now();
    while (Date.now() - started < 12000) {
      const busy = document.querySelector('.canvas[aria-busy="true"]');
      const enriching = document.querySelector('.status-quality')?.getAttribute('data-enriching') === 'true';
      if (!busy && !enriching) return;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error('Composition projection did not settle');
  };

  const mode = await waitFor('button[data-mode-id="composition"]');
  if (mode.disabled) throw new Error('Composition mode is not available for the loaded workspace');
  mode.click();
  await waitFor('button[data-mode-id="composition"].active');
  await waitForSettled();
  const reset = document.querySelector('.composition-clear:not(:disabled)');
  if (reset) {
    reset.click();
    await waitFor('.composition-selection-empty');
  }

  let context = await waitFor('.product-context-browser');
  if (window.innerWidth <= 820 && !context.classList.contains('compact-open')) {
    (await waitFor('.product-context-toggle')).click();
    context = await waitFor('.product-context-browser.compact-open');
  }
  const codeOptions = [...context.querySelectorAll('[data-context-id^="code:"]')];
  const dbOptions = [...context.querySelectorAll('[data-context-id^="db:table:"]')];
  const codeOption = codeOptions.find((option) => option.textContent?.includes('loadOrder'));
  const dbOption = dbOptions.find((option) => option.textContent?.includes('orders'));
  const codeInput = codeOption?.querySelector('input[type="checkbox"]');
  const dbInput = dbOption?.querySelector('input[type="checkbox"]');
  if (!codeInput || !dbInput) throw new Error('Composition smoke needs loadOrder and orders from the semantic fixture');
  codeInput.click();
  if (!codeInput.checked) throw new Error('First composition subject was not retained');
  dbInput.click();
  await new Promise((resolve) => setTimeout(resolve, 0));
  await waitForSettled();

  const closeContext = document.querySelector('.product-context-close');
  if (window.innerWidth <= 820) {
    if (!closeContext || getComputedStyle(closeContext).display === 'none') throw new Error('Compact subject list has no visible close control');
    closeContext.click();
    const closeStarted = Date.now();
    while (document.querySelector('.product-context-browser.compact-open') && Date.now() - closeStarted < 2000) {
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    if (document.querySelector('.product-context-browser.compact-open')) throw new Error('Compact subject list did not close');
  }
  const toolbar = await waitFor('.composition-toolbar');
  const chips = toolbar.querySelectorAll('.composition-targets > button');
  if (chips.length !== 2) throw new Error(`Expected 2 composition chips, got ${chips.length}`);
  const viewButtons = [...toolbar.querySelectorAll('.composition-view-switch button')];
  if (viewButtons.length !== 4) throw new Error(`Expected 4 relationship views, got ${viewButtons.length}`);
  const dataView = viewButtons.find((button) => button.textContent?.trim() === '\uB370\uC774\uD130');
  if (!dataView) throw new Error('Data relationship view is missing');
  dataView.click();
  await new Promise((resolve) => setTimeout(resolve, 0));
  await waitForSettled();
  const selectedDataView = await waitFor('.composition-view-switch button.active[aria-pressed="true"]');
  if (selectedDataView?.getAttribute('aria-pressed') !== 'true') throw new Error('Data relationship view did not remain selected');
  if (document.querySelector('.composition-selection-empty')) throw new Error('Composition returned to the target-selection empty state');
  if (document.querySelector('.setup-empty')) throw new Error('Composition did not produce a relationship map');
  const cards = document.querySelectorAll('.at-map-surface .at-card');
  if (cards.length < 2) throw new Error(`Expected at least 2 composition nodes, got ${cards.length}`);
  const relationRows = [...document.querySelectorAll('.at-edge-row')];
  const readRelation = relationRows.find((row) => row.textContent?.includes('\u0044\u0042 \uC870\uD68C'));
  if (!readRelation) throw new Error('Confirmed DB read relationship is missing from the composition ledger');
  if (!readRelation.classList.contains('confirmed')) throw new Error('DB read relationship is not marked confirmed');
  if (document.querySelectorAll('.product-context-option input:checked').length !== 2) {
    throw new Error('Composition subjects were lost after changing HOW');
  }
  const toolbarRect = toolbar.getBoundingClientRect();
  if (toolbarRect.left < -1 || toolbarRect.right > window.innerWidth + 1) throw new Error('Composition toolbar is clipped');
  if (document.documentElement.scrollWidth > window.innerWidth + 2) throw new Error('Root document overflows horizontally');
  return { ok: true, labels: [`subjects:${chips.length}`, `nodes:${cards.length}`, `relations:${relationRows.length}`, `views:${viewButtons.length}`, 'how:data', 'proof:confirmed-read'] };
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
elseif ($Scenario -eq "stable-navigation") {
  $stableNavigationExpression
}
elseif ($Scenario -eq "semantic-composition") {
  $compositionExpression
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
