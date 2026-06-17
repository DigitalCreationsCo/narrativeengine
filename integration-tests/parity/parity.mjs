import { createBlock, generateCandidate, renderLoreSummary } from "../../typescript/narrativeengine/dist/index.js";

const block = createBlock("intro", "A signal appears in the archive.");
const lore = { id: "lore-1", title: "Archive Signal", blocks: [block] };
const config = { temperature: 0.7, max_candidates: 4, seed: 7 };
const candidate = generateCandidate(lore, config);

process.stdout.write(JSON.stringify({
  block,
  candidate,
  summary: renderLoreSummary(lore),
}) + "\n");

