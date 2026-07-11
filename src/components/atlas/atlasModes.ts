import { Braces, Layers3, Radar, Share2 } from "lucide-react";
import type { ComponentType } from "react";

type Icon = ComponentType<{ size?: number }>;

export const atlasModes: {
  id: "atlas" | "dependencies" | "impact" | "api";
  icon: Icon;
  title: string;
  text: string;
}[] = [
  { id: "atlas", icon: Layers3, title: "전체 구조", text: "API·코드·DB 요약" },
  { id: "api", icon: Braces, title: "API가 닿는 코드", text: "라우트 → 코드" },
  { id: "dependencies", icon: Share2, title: "테이블 연결", text: "코드 후보 · PK/FK" },
  { id: "impact", icon: Radar, title: "컬럼 변경 범위", text: "직접/후보 근거 분리" },
];
