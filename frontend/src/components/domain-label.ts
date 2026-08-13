import type { DomainNode } from '../domain'

export function featureLabel(domain: DomainNode, index: number) {
  const labels = ['목록 조회 및 검색', '상세 정보 조회', '생성 요청 처리', '상태 변경 동기화']
  return `${domain.name} · ${labels[index]}`
}
