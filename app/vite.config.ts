import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

// Tauri expects a fixed dev port, and keeping the screen uncleared leaves Rust errors visible.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  // `@/…` imports (shadcn/ui's convention) come from the paths in tsconfig.json.
  resolve: { tsconfigPaths: true },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  // three.js is ~1 MB minified and dockview ~0.4 MB; a desktop app loads them from disk, so no
  // code splitting.
  build: { target: 'esnext', chunkSizeWarningLimit: 2000 },
});
