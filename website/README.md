# optionMusic — website

Standalone landing site (Vite + React + TS + Tailwind v4). Quiet B&W editorial.

Single-page app: `src/pages/HomePage.tsx` under `src/components/layout/Shell.tsx`, shared design-system components in `shared/` (synced from the optionDesign monorepo).

## Commands

```bash
cd website
bun install
bun run dev       # Vite dev server on http://localhost:1421
bun run build     # typecheck and build dist/
bun run test      # build and run tests
bun run preview   # preview dist on :1421
```

Deployed on Vercel (`vercel.json`): static SPA, no middleware.
