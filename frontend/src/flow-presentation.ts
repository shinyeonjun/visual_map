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
  return kind === 'return' || kind === 'throw'
}
