import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

/** Everything the page loads comes from its own origin, so the browser
 * refuses to send a trace anywhere. */
const CONTENT_SECURITY_POLICY = [
  "default-src 'self'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "style-src 'self'",
  "img-src 'self'",
  "font-src 'self'",
  "connect-src 'self'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
].join("; ");

/** Adds the policy to built pages only: the dev server needs inline scripts
 * and a websocket. As a meta tag, it covers both the hosted site and the copy
 * `peel view` embeds. */
function contentSecurityPolicy(): Plugin {
  return {
    name: "content-security-policy",
    apply: "build",
    transformIndexHtml: () => [
      {
        tag: "meta",
        attrs: {
          "http-equiv": "Content-Security-Policy",
          content: CONTENT_SECURITY_POLICY,
        },
        injectTo: "head-prepend",
      },
    ],
  };
}

export default defineConfig({
  plugins: [react(), contentSecurityPolicy()],
  // Relative asset paths, so the build works under `peel view` and any
  // hosting path.
  base: "./",
  clearScreen: false,
  server: {
    port: 1430,
    strictPort: true,
  },
  build: {
    target: "esnext",
  },
});
