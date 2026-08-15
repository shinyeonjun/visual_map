export type FlowStepKind =
  | 'call'
  | 'condition'
  | 'switch'
  | 'loop'
  | 'return'
  | 'throw'
  | 'dynamicBoundary'
  | string

export const STEP_KIND_META: Record<string, { label: string; tone: string; symbol: string }> = {
  call: { label: '호출', tone: 'call', symbol: '→' },
  condition: { label: '분기', tone: 'branch', symbol: '◇' },
  switch: { label: '선택', tone: 'branch', symbol: '⋮' },
  loop: { label: '반복', tone: 'loop', symbol: '↻' },
  return: { label: '반환', tone: 'return', symbol: '↩' },
  throw: { label: '예외', tone: 'throw', symbol: '!' },
  dynamicBoundary: { label: '동적', tone: 'dynamic', symbol: '∿' },
}

export function stepKindLabel(kind: string): string {
  return STEP_KIND_META[kind]?.label ?? kind.toUpperCase()
}

export function stepKindTone(kind: string): string {
  return STEP_KIND_META[kind]?.tone ?? 'default'
}

export function stepKindSymbol(kind: string): string {
  return STEP_KIND_META[kind]?.symbol ?? '•'
}

export function isTerminalKind(kind: string): boolean {
  return kind === 'return' || kind === 'throw' || kind === 'break' || kind === 'continue'
}

export const EDGE_KIND_META: Record<string, { label: string; tone: string }> = {
  sequential: { label: '다음', tone: 'sequential' },
  trueBranch: { label: 'true', tone: 'branch' },
  falseBranch: { label: 'false', tone: 'branch' },
  loopBody: { label: '반복', tone: 'loop' },
  loopBack: { label: '되돌아감', tone: 'loop' },
  call: { label: '호출', tone: 'call' },
  return: { label: '반환', tone: 'return' },
  exception: { label: '예외', tone: 'throw' },
  dynamic: { label: '동적', tone: 'dynamic' },
}

export function edgeKindLabel(kind: string, label?: string | null): string {
  if (label && label.trim()) return label
  return EDGE_KIND_META[kind]?.label ?? kind
}

export function edgeKindTone(kind: string): string {
  return EDGE_KIND_META[kind]?.tone ?? 'default'
}

export function isBackEdge(kind: string): boolean {
  return kind === 'loopBack'
}

export function isExceptionEdge(kind: string): boolean {
  return kind === 'exception'
}
