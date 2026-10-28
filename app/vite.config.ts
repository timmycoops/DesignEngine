import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Tauri expects a fixed dev port, and keeping the screen uncleared leaves Rust errors visible.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  // three.js alone is ~1 MB minified; a desktop app loads it from disk, so no code splitting.
  build: { target: 'esnext', chunkSizeWarningLimit: 1500 },
});
