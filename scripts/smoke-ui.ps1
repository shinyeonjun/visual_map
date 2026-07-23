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

$uiHelpers = @'
  const sleep = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));
  const waitFor = async (selector, timeout = 10000) => {
    const started = Date.now();
    while (Date.now() - started < timeout) {
      const node = document.querySelector(selector);
      if (node) return node;
      await sleep(40);
    }
    throw new Error('Timed out waiting for ' + selector);
  };
  const waitUntil = async (check, message, timeout = 10000) => {
    const started = Date.now();
    while (Date.now() - started < timeout) {
      const value = check();
      if (value) return value;
      await sleep(40);
    }
    throw new Error(message);
  };
  const waitForIdle = async () => {
    await sleep(20);
    await waitUntil(
      () => (
        !document.querySelector('.answer-canvas[aria-busy="true"], .canvas[aria-busy="true"]') &&
        !document.querySelector('.answer-refreshing') &&
        document.querySelector('.status-quality')?.getAttribute('data-enriching') !== 'true'
      ),
      'Analysis transition did not settle',
      15000,
    );
  };
  const showSurface = async (surface) => {
    const shell = await waitFor('.product-shell');
    if (shell.getAttribute('data-surface') !== surface) {
      const label = surface === 'answers' ? '\uB2F5 \uBCF4\uAE30' : '\uC804\uCCB4 \uAD6C\uC870';
      const button = await waitFor('.product-view-switch button[aria-label="' + label + '"]');
      if (button.disabled) throw new Error(label + ' is disabled');
      button.click();
    }
    await waitUntil(
      () => document.querySelector('.product-shell')?.getAttribute('data-surface') === surface,
      'Surface did not change to ' + surface,
      15000,
    );
    await waitForIdle();
    return document.querySelector('.product-shell');
  };
  const showAdvancedMode = async (mode) => {
    await showSurface('advanced');
    const button = await waitFor('button[data-mode-id="' + mode + '"]');
    if (button.disabled) throw new Error(mode + ' mode is unavailable');
    if (!button.classList.contains('active')) {
      button.click();
      await waitUntil(
        () => document.querySelector('button[data-mode-id="' + mode + '"].active'),
        mode + ' mode did not become active',
        15000,
      );
      await waitForIdle();
    }
    return button;
  };
  const findTargetButton = (targetId) => [...document.querySelectorAll('.target-list button[data-target-id]')]
    .find((button) => button.getAttribute('data-target-id') === targetId);
  const selectTarget = async (kind, preferredText = []) => {
    await showSurface('answers');
    const tab = await waitFor('button[data-target-kind="' + kind + '"]');
    if (tab.getAttribute('aria-selected') !== 'true') {
      tab.click();
      await waitUntil(() => tab.getAttribute('aria-selected') === 'true', kind + ' target tab did not become active');
    }
    const targets = await waitUntil(
      () => {
        const items = [...document.querySelectorAll('.target-list button[data-target-id]')];
        return items.length > 0 ? items : null;
      },
      kind + ' target list is empty',
    );
    const needles = Array.isArray(preferredText) ? preferredText : [preferredText];
    const target = needles
      .map((needle) => targets.find((item) => item.textContent?.toLocaleLowerCase().includes(String(needle).toLocaleLowerCase())))
      .find(Boolean) ?? targets[0];
    const targetId = target.getAttribute('data-target-id');
    if (!targetId) throw new Error('Target has no stable identity');
    target.click();
    const expectedMode = {
      api: 'api-flow',
      code: 'search-focus',
      table: 'table-usage',
      column: 'column-impact',
    }[kind];
    await waitUntil(
      () => {
        const answer = document.querySelector('.answer-canvas[data-answer-mode]');
        return answer?.getAttribute('data-answer-mode') === expectedMode &&
          answer?.getAttribute('data-answer-focus') === targetId;
      },
      kind + ' answer did not commit for ' + targetId,
      15000,
    );
    await waitForIdle();
    return { targetId, title: target.querySelector('strong')?.textContent?.trim() ?? target.textContent?.trim() ?? targetId };
  };
  const assertSurfaceControls = () => {
    const controls = [...document.querySelectorAll('.product-view-switch button')];
    const labels = controls.map((button) => button.getAttribute('aria-label'));
    if (labels.join('|') !== '\uB2F5 \uBCF4\uAE30|\uC804\uCCB4 \uAD6C\uC870') {
      throw new Error('Unexpected primary surfaces: ' + labels.join('|'));
    }
  };
  const assertNoOverflow = () => {
    if (document.documentElement.scrollWidth > window.innerWidth + 2) {
      throw new Error('Root document overflows horizontally');
    }
  };
'@

function Add-UiHelpers([string]$Expression) {
  return $Expression.Replace("__UI_HELPERS__", $uiHelpers)
}

$atlasExpression = Add-UiHelpers @'
(async () => {
__UI_HELPERS__
  assertSurfaceControls();
  await showAdvancedMode('atlas');
  const modeIds = [...document.querySelectorAll('button[data-mode-id]')]
    .map((button) => button.getAttribute('data-mode-id'));
  if (modeIds.join('|') !== 'atlas|composition') {
    throw new Error('Advanced navigation drifted: ' + modeIds.join('|'));
  }

  if (!document.querySelector('.at-domain-card')) {
    const back = document.querySelector('button[data-atlas-action="overview"]');
    if (back && !back.disabled) {
      back.click();
      await waitForIdle();
    }
  }
  const card = await waitFor('.at-domain-card');
  card.click();
  await waitFor('.at-domain-band');
  const bands = [...document.querySelectorAll('.at-domain-band')];
  const labels = bands.map((band) => band.getAttribute('data-domain-band') ?? '');
  if (labels.join('|') !== '1|2|3') {
    throw new Error('Unexpected domain detail order: ' + labels.join('|'));
  }
  assertNoOverflow();
  return { ok: true, labels: ['surface:structure', 'modes:2', 'bands:' + labels.join('-')] };
})()
'@

$apiExpression = Add-UiHelpers @'
(async () => {
__UI_HELPERS__
  assertSurfaceControls();
  const selected = await selectTarget('api', ['/orders/{order_id}', '/orders']);
  const answer = await waitFor('.answer-canvas[data-answer-mode="api-flow"]');
  if (!answer.querySelector('.answer-header') || !answer.querySelector('.answer-verdicts')) {
    throw new Error('API answer is missing its conclusion or evidence summary');
  }
  const steps = [...answer.querySelectorAll('.answer-flow > li')];
  if (steps.length === 0) throw new Error('API answer has no confirmed reading step');
  const candidates = answer.querySelector('details.answer-candidates');
  if (candidates?.open) throw new Error('Candidate evidence opened before the confirmed answer');
  if (document.querySelector('button[data-mode-id="api"]')) {
    throw new Error('Removed graph-first API mode returned to the primary answer surface');
  }

  const selectableStep = steps.map((step) => step.querySelector('button:not(:disabled)')).find(Boolean);
  if (!selectableStep) throw new Error('API flow exposes no selectable evidence step');
  selectableStep.click();
  await waitUntil(
    () => document.querySelector('.product-workspace')?.classList.contains('inspector-visible'),
    'Selecting an API step did not open evidence',
  );
  const inspector = await waitFor('.inspector');
  if (inspector.querySelectorAll('.inspector-section').length < 3) {
    throw new Error('API evidence panel is incomplete');
  }
  const evidenceHeadings = [...inspector.querySelectorAll('.inspector-section > header > strong')]
    .map((heading) => heading.textContent?.trim());
  if (evidenceHeadings[0] !== '\uC120\uD0DD') {
    throw new Error('Answer evidence repeats the main summary: ' + evidenceHeadings.join('|'));
  }
  const repeatedSummary = inspector.querySelector('.answer-summary p, .answer-summary .answer-lead');
  if (repeatedSummary && getComputedStyle(repeatedSummary).display !== 'none') {
    throw new Error('Answer evidence repeats conclusion copy from the main answer');
  }
  const evidenceList = inspector.querySelector('.inspector-evidence-list');
  if (evidenceList && ['auto', 'scroll'].includes(getComputedStyle(evidenceList).overflowY)) {
    throw new Error('Answer evidence has a nested vertical scroll area');
  }
  const sourceActions = [...inspector.querySelectorAll('[data-source-action]')]
    .map((button) => button.getAttribute('data-source-action'));
  if (!sourceActions.includes('reveal')) throw new Error('API evidence has no source location action');
  const evidencePanel = await waitFor('.evidence-panel');
  if (window.innerWidth <= 820 && getComputedStyle(evidencePanel).display === 'none') {
    throw new Error('Compact evidence overlay did not open');
  }
  assertNoOverflow();
  return { ok: true, labels: ['surface:answer', 'target:' + selected.title, 'steps:' + steps.length, 'evidence:open'] };
})()
'@

$impactExpression = Add-UiHelpers @'
(async () => {
__UI_HELPERS__
  const selected = await selectTarget('column', ['status']);
  const answer = await waitFor('.answer-canvas[data-answer-mode="column-impact"]');
  if (!answer.querySelector('.answer-header') || !answer.querySelector('.answer-verdicts')) {
    throw new Error('Column impact answer is missing its conclusion');
  }
  const intent = await waitFor('.answer-change-intent select');
  if (!answer.querySelector('.answer-section')) throw new Error('Column impact has no direct-impact section');
  const candidates = answer.querySelector('details.answer-candidates');
  if (candidates?.open) throw new Error('Impact candidates opened before direct evidence');

  const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set;
  if (!setter) throw new Error('Select value setter is unavailable');
  setter.call(intent, 'drop');
  intent.dispatchEvent(new Event('change', { bubbles: true }));
  await waitForIdle();
  await waitUntil(
    () => document.querySelector('.answer-canvas')?.getAttribute('data-answer-focus') === selected.targetId,
    'Changing impact intent lost the selected column',
  );
  if (intent.value !== 'drop') throw new Error('Column change intent did not remain selected');
  assertNoOverflow();
  return { ok: true, labels: ['surface:answer', 'target:' + selected.title, 'intent:drop', candidates ? 'candidates:collapsed' : 'candidates:none'] };
})()
'@

$sourceJumpExpression = Add-UiHelpers @'
(async () => {
__UI_HELPERS__
  const exerciseReveal = __EXERCISE_REVEAL__;
  const selected = await selectTarget('code', ['load_order', 'loadOrder', 'find_order_by_id']);
  document.querySelector('.inspector-close')?.click();
  await sleep(40);
  const target = findTargetButton(selected.targetId);
  if (!target) throw new Error('Selected code target disappeared from navigation');
  target.click();
  await waitUntil(
    () => document.querySelector('.product-workspace')?.classList.contains('inspector-visible'),
    'Selecting the active code target did not open evidence',
  );
  const inspector = await waitFor('.inspector');
  const actions = [...inspector.querySelectorAll('[data-source-action]')]
    .map((node) => node.getAttribute('data-source-action') || '');
  const expected = ['vscode', 'cursor', 'reveal'];
  if (actions.join('|') !== expected.join('|')) {
    throw new Error('Unexpected source actions: ' + actions.join('|'));
  }

  const add = await waitFor('[data-investigation-action="add"]');
  if (!add.disabled) add.click();
  const tray = await waitFor('.investigation-tray');
  const item = await waitFor('.investigation-item');
  const toggle = await waitFor('[data-investigation-action="toggle"]');
  const checkedBefore = toggle.getAttribute('aria-pressed');
  toggle.click();
  await waitUntil(
    () => toggle.getAttribute('aria-pressed') !== checkedBefore,
    'Investigation checked state did not update',
    2000,
  );
  if (!tray.querySelector('[data-investigation-storage="source-location-evidence-check-state-only"]')) {
    throw new Error('Investigation privacy boundary is missing');
  }
  const evidence = item.querySelector('code');
  if (!evidence?.textContent?.trim()) throw new Error('Investigation evidence ID is missing');
  const storageKey = Object.keys(localStorage).find((key) => key.startsWith('backend-visual-map:investigation:v1:'));
  const stored = storageKey ? JSON.parse(localStorage.getItem(storageKey) || '[]') : [];
  const allowedKeys = ['checked', 'column', 'evidenceId', 'line', 'path'];
  if (!stored.length || Object.keys(stored[0]).sort().join('|') !== allowedKeys.join('|')) {
    throw new Error('Investigation storage contains missing or unexpected fields');
  }
  const copy = await waitFor('[data-investigation-action="copy"]');
  copy.click();
  await waitUntil(
    () => copy.getAttribute('data-copy-state') !== 'idle',
    'Investigation Markdown export did not finish',
    2000,
  );
  if (exerciseReveal) {
    inspector.querySelector('[data-source-action="reveal"]').click();
    await waitFor('[data-source-status="success"]');
  }
  tray.scrollIntoView({ block: 'center' });
  assertNoOverflow();
  return { ok: true, labels: ['target:' + selected.title, ...actions, 'investigation'] };
})()
'@
$sourceJumpExpression = $sourceJumpExpression.Replace(
  "__EXERCISE_REVEAL__",
  $ExerciseReveal.IsPresent.ToString().ToLowerInvariant()
)

$largeRepoExpression = Add-UiHelpers @'
(async () => {
__UI_HELPERS__
  await showSurface('answers');
  const codeTab = await waitFor('button[data-target-kind="code"]');
  if (codeTab.getAttribute('aria-selected') !== 'true') {
    codeTab.click();
    await waitUntil(() => codeTab.getAttribute('aria-selected') === 'true', 'Code target tab did not activate');
  }
  const targetItems = await waitUntil(
    () => {
      const items = [...document.querySelectorAll('.target-list button[data-target-id]')];
      return items.length ? items : null;
    },
    'Code target list is empty',
  );
  if (targetItems.length > 100) throw new Error('Target navigator exceeds 100 visible code items: ' + targetItems.length);

  const title = targetItems[0].querySelector('strong')?.textContent?.trim() ?? 'order';
  const tokens = title.match(/[A-Za-z_][A-Za-z0-9_]*/g) ?? ['order'];
  const searchTerm = tokens[tokens.length - 1];
  const input = await waitFor('#global-inventory-search');
  input.focus();
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
  if (!setter) throw new Error('Input value setter is unavailable');
  setter.call(input, '');
  input.dispatchEvent(new Event('input', { bubbles: true }));
  await sleep(50);
  const searchStarted = performance.now();
  setter.call(input, searchTerm);
  input.dispatchEvent(new Event('input', { bubbles: true }));
  await waitFor('.search-popover .search-result', 750);
  const searchMs = Math.round(performance.now() - searchStarted);
  if (searchMs > 750) throw new Error('Search exceeded 750ms: ' + searchMs + 'ms');
  input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));

  await showAdvancedMode('atlas');
  if (!document.querySelector('.at-domain-card')) {
    const back = document.querySelector('button[data-atlas-action="overview"]');
    if (back && !back.disabled) {
      back.click();
      await waitForIdle();
    }
  }
  await waitFor('.at-domain-card');
  const cards = document.querySelectorAll('.at-domain-card').length;
  if (cards > 40) throw new Error('Overview exceeds 40 cards: ' + cards);

  let diagnosticsText = null;
  const originalExecCommand = document.execCommand;
  document.execCommand = (command) => {
    if (command === 'copy' && document.activeElement instanceof HTMLTextAreaElement) {
      diagnosticsText = document.activeElement.value;
      return true;
    }
    return originalExecCommand ? originalExecCommand.call(document, command) : false;
  };
  const diagnostics = await waitFor('[data-diagnostics-action="copy"]');
  diagnostics.click();
  await waitUntil(() => diagnosticsText, 'Diagnostics export did not reach the clipboard boundary', 2000);
  document.execCommand = originalExecCommand;
  const bundle = JSON.parse(diagnosticsText);
  const serialized = JSON.stringify(bundle);
  const forbiddenKeys = ['workspaceId', 'workspaceName', 'repoPath', 'path', 'executable', 'engineDir', 'details', 'error'];
  if (forbiddenKeys.some((key) => serialized.includes('"' + key + '"'))) {
    throw new Error('Diagnostics export contains a forbidden identifying or error field');
  }
  if (bundle.schemaVersion !== 1 || typeof bundle.projection.elapsedMs !== 'number') {
    throw new Error('Diagnostics export is missing its schema or projection timing');
  }
  assertNoOverflow();
  return { ok: true, labels: ['targets:' + targetItems.length, 'cards:' + cards, 'search:' + searchMs + 'ms', 'diagnostics:redacted'] };
})()
'@

$stableNavigationExpression = Add-UiHelpers @'
(async () => {
__UI_HELPERS__
  assertSurfaceControls();
  const selected = await selectTarget('code', ['load_order', 'loadOrder']);
  const codeTab = await waitFor('button[data-target-kind="code"]');
  const tableTab = await waitFor('button[data-target-kind="table"]');
  codeTab.focus();
  codeTab.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
  await waitUntil(
    () => tableTab.getAttribute('aria-selected') === 'true' && document.activeElement === tableTab,
    'Target tabs did not move right with the keyboard',
  );
  tableTab.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }));
  await waitUntil(
    () => codeTab.getAttribute('aria-selected') === 'true' && document.activeElement === codeTab,
    'Target tabs did not move left with the keyboard',
  );
  const targetButtons = [...document.querySelectorAll('.target-list button[data-target-id]')];
  const targetTabStops = targetButtons.filter((button) => button.tabIndex >= 0);
  if (targetTabStops.length !== 1) {
    throw new Error('Target list exposes ' + targetTabStops.length + ' tab stops');
  }
  const targetStart = targetTabStops[0];
  const targetStartIndex = targetButtons.indexOf(targetStart);
  const targetNextIndex = targetStartIndex < targetButtons.length - 1
    ? targetStartIndex + 1
    : targetStartIndex - 1;
  const targetNext = targetButtons[targetNextIndex];
  const targetMoveKey = targetNextIndex > targetStartIndex ? 'ArrowDown' : 'ArrowUp';
  const answerFocusBeforeTargetMove = document.querySelector('.answer-canvas')?.getAttribute('data-answer-focus');
  targetStart.focus();
  targetStart.dispatchEvent(new KeyboardEvent('keydown', { key: targetMoveKey, bubbles: true }));
  await waitUntil(
    () => document.activeElement === targetNext,
    'Target list did not move focus with the keyboard',
  );
  await sleep(80);
  if (targetButtons.filter((button) => button.tabIndex >= 0).length !== 1 || targetNext.tabIndex !== 0) {
    throw new Error('Target list did not preserve one roving tab stop');
  }
  if (document.querySelector('.answer-refreshing')) {
    throw new Error('Target keyboard navigation restarted analysis');
  }
  if (document.querySelector('.answer-canvas')?.getAttribute('data-answer-focus') !== answerFocusBeforeTargetMove) {
    throw new Error('Target keyboard navigation changed the current answer');
  }
  const switcher = await waitFor('.product-view-switch');
  const switcherBefore = switcher.getBoundingClientRect();
  const workspace = await waitFor('.product-workspace');

  await showAdvancedMode('atlas');
  const modeIds = [...document.querySelectorAll('button[data-mode-id]')]
    .map((button) => button.getAttribute('data-mode-id'));
  if (modeIds.join('|') !== 'atlas|composition') {
    throw new Error('Advanced surface exposes unexpected modes: ' + modeIds.join('|'));
  }

  const contextToggle = document.querySelector('.product-context-toggle');
  if (contextToggle && getComputedStyle(contextToggle).display !== 'none') {
    contextToggle.click();
    const compactContext = await waitFor('.product-context-browser.compact-open');
    if (!compactContext.querySelector('.product-context-list') || !compactContext.querySelector('.product-context-filter')) {
      throw new Error('Compact structure context is incomplete');
    }
    compactContext.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await waitUntil(
      () => !document.querySelector('.product-context-browser.compact-open'),
      'Compact context did not close with Escape',
      2000,
    );
    if (document.activeElement !== contextToggle) {
      throw new Error('Compact context control did not regain focus');
    }
  }

  await showSurface('answers');
  await waitUntil(
    () => {
      const answer = document.querySelector('.answer-canvas[data-answer-mode="search-focus"]');
      return answer?.getAttribute('data-answer-focus') === selected.targetId;
    },
    'Returning to answers did not restore the last selected target',
    15000,
  );
  const restoredTarget = findTargetButton(selected.targetId);
  if (!restoredTarget || restoredTarget.getAttribute('aria-current') !== 'true') {
    throw new Error('Restored target is not marked current');
  }
  const switcherAfter = switcher.getBoundingClientRect();
  const switcherShift = Math.max(
    Math.abs(switcherBefore.left - switcherAfter.left),
    Math.abs(switcherBefore.top - switcherAfter.top),
    Math.abs(switcherBefore.width - switcherAfter.width),
    Math.abs(switcherBefore.height - switcherAfter.height),
  );
  if (switcherShift > 1) throw new Error('Primary surface controls moved by ' + switcherShift + 'px');

  restoredTarget.click();
  await sleep(80);
  if (workspace.classList.contains('inspector-visible')) {
    throw new Error('Current target opened evidence through a hidden alternate action');
  }
  if (document.querySelector('.answer-refreshing')) throw new Error('Current target restarted analysis');
  const evidenceAction = await waitFor('.answer-evidence-action');
  evidenceAction.click();
  await waitUntil(
    () => workspace.classList.contains('inspector-visible'),
    'Explicit evidence action did not open evidence',
  );
  if (document.querySelector('.answer-refreshing')) throw new Error('Evidence selection restarted analysis');
  const inspectorBody = await waitFor('.inspector-scroll-body');
  const sourceAction = await waitFor('.inspector [data-source-action="reveal"]');
  const nextCheck = document.querySelector('.inspector > .inspector-section:last-child');
  if (!nextCheck) throw new Error('Evidence footer is missing');
  sourceAction.scrollIntoView({ block: 'end' });
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  const sourceRect = sourceAction.getBoundingClientRect();
  const bodyRect = inspectorBody.getBoundingClientRect();
  const nextRect = nextCheck.getBoundingClientRect();
  if (sourceRect.top < bodyRect.top - 1 || sourceRect.bottom > bodyRect.bottom + 1 || sourceRect.bottom > nextRect.top) {
    throw new Error('Fixed evidence footer covers source actions');
  }
  const closeSelection = await waitFor('.inspector-close');
  closeSelection.click();
  await waitUntil(() => !workspace.classList.contains('inspector-visible'), 'Evidence panel did not clear', 2000);
  const evidencePanel = await waitFor('.evidence-panel');
  if (window.innerWidth <= 820 && getComputedStyle(evidencePanel).display !== 'none') {
    throw new Error('Compact evidence overlay stayed open after clearing');
  }
  if (window.innerWidth > 820 && getComputedStyle(evidencePanel).display === 'none') {
    throw new Error('Desktop evidence column collapsed after clearing');
  }

  const sourceTrigger = await waitFor('.source-manager-trigger');
  sourceTrigger.click();
  const sourceManager = await waitFor('.source-manager');
  await waitUntil(() => sourceManager.contains(document.activeElement), 'Source manager did not receive focus', 2000);
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }));
  await sleep(80);
  if (!sourceManager.contains(document.activeElement) || document.activeElement?.id === 'global-inventory-search') {
    throw new Error('Ctrl+K moved focus behind the source manager');
  }
  sourceManager.querySelector('.source-manager-header .tool')?.click();
  await waitUntil(() => !document.querySelector('.source-manager'), 'Source manager did not close', 2000);
  assertNoOverflow();
  return { ok: true, labels: ['surfaces:2', 'advanced-modes:2', 'tabs:keyboard', 'targets:roving', 'target:restored', 'switcher:fixed', 'evidence:stable'] };
})()
'@

$compositionExpression = Add-UiHelpers @'
(async () => {
__UI_HELPERS__
  await showAdvancedMode('composition');
  const reset = document.querySelector('.composition-clear:not(:disabled)');
  if (reset) {
    reset.click();
    await waitFor('.composition-selection-empty');
  }

  let context = await waitFor('.product-context-browser');
  let compactOpened = false;
  if (getComputedStyle(context).display === 'none') {
    const toggle = await waitFor('.product-context-toggle');
    toggle.click();
    context = await waitFor('.product-context-browser.compact-open');
    compactOpened = true;
  }
  const codeOptions = [...context.querySelectorAll('[data-context-id^="code:"]')];
  const dbOptions = [...context.querySelectorAll('[data-context-id^="db:table:"]')];
  const codeOption = codeOptions.find((option) => option.textContent?.includes('loadOrder'));
  const dbOption = dbOptions.find((option) => option.textContent?.includes('orders'));
  const codeInput = codeOption?.querySelector('input[type="checkbox"]');
  const dbInput = dbOption?.querySelector('input[type="checkbox"]');
  if (!codeInput || !dbInput) {
    throw new Error('Composition smoke needs loadOrder and orders from the semantic fixture');
  }
  codeInput.click();
  if (!codeInput.checked) throw new Error('First composition subject was not retained');
  dbInput.click();
  await waitForIdle();

  if (compactOpened) {
    const closeContext = await waitFor('.product-context-close');
    closeContext.click();
    await waitUntil(
      () => !document.querySelector('.product-context-browser.compact-open'),
      'Compact subject list did not close',
      2000,
    );
  }
  const toolbar = await waitFor('.composition-toolbar');
  const chips = toolbar.querySelectorAll('.composition-targets > button');
  if (chips.length !== 2) throw new Error('Expected 2 composition chips, got ' + chips.length);
  const viewButtons = [...toolbar.querySelectorAll('.composition-view-switch button')];
  if (viewButtons.length !== 4) throw new Error('Expected 4 relationship views, got ' + viewButtons.length);
  const dataView = viewButtons.find((button) => button.textContent?.trim() === '\uB370\uC774\uD130');
  if (!dataView) throw new Error('Data relationship view is missing');
  dataView.click();
  await waitForIdle();
  if (dataView.getAttribute('aria-pressed') !== 'true') throw new Error('Data relationship view did not remain selected');
  if (document.querySelector('.composition-selection-empty') || document.querySelector('.setup-empty')) {
    throw new Error('Composition did not produce a relationship answer');
  }
  const cards = document.querySelectorAll('.at-map-surface .at-card');
  if (cards.length < 2) throw new Error('Expected at least 2 composition nodes, got ' + cards.length);
  const relationRows = [...document.querySelectorAll('.at-edge-row')];
  const readRelation = relationRows.find((row) => row.textContent?.includes('DB \uC870\uD68C'));
  if (!readRelation) throw new Error('Confirmed DB read relationship is missing');
  if (!readRelation.classList.contains('confirmed')) throw new Error('DB read relationship is not marked confirmed');
  if (document.querySelectorAll('.product-context-option input:checked').length !== 2) {
    throw new Error('Composition subjects were lost after changing the relationship view');
  }
  const toolbarRect = toolbar.getBoundingClientRect();
  if (toolbarRect.left < -1 || toolbarRect.right > window.innerWidth + 1) {
    throw new Error('Composition toolbar is clipped');
  }
  assertNoOverflow();
  return { ok: true, labels: ['subjects:2', 'nodes:' + cards.length, 'relations:' + relationRows.length, 'views:4', 'proof:confirmed-read'] };
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
  Write-Output "PASS $($Scenario): $($result.value.labels -join ' -> ')"
}
finally {
  $resolved = [IO.Path]::GetFullPath($expressionPath)
  if ($resolved.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) {
    Remove-Item -LiteralPath $resolved -Force -ErrorAction SilentlyContinue
  }
}
