import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  // Tauri Rust 빌드 산출물은 Vite HMR 대상이 아니다.
  // Windows에서는 Cargo가 생성한 .exe를 감시하면 EBUSY 잠금 오류가 발생할 수 있다.
  server: {
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
})
