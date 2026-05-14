# codex-fleet recodee-internal extras

This file documents tooling that lives inside the [recodee](https://github.com/recodeee/recodee)
product (codex-gpu-embedder, the workspace-build `colony` CLI, GPU embedding
provider) and is only relevant when running the fleet *against* a recodee
checkout. Public consumers of codex-fleet don't need any of this.

It used to live inline in `skills/codex-fleet/SKILL.md`. Moved here during
the extraction so the public SKILL.md isn't cluttered with absolute paths
into a private project tree.

---

## `curl /healthz` returns `cpu-stub` instead of `ort-cuda-minilm`

Model files are missing where the embedder expects. Quick fix (recodee-only):

```bash
DST="$HOME/.cache/codex-gpu-embedder/all-MiniLM-L6-v2"
mkdir -p "$DST"
cp ~/.jcode/models/all-MiniLM-L6-v2/{model.onnx,tokenizer.json} "$DST/"
pkill -f codex-gpu-embedder
export ORT_DYLIB_PATH=$HOME/.local/lib/python3.10/site-packages/onnxruntime/capi/libonnxruntime.so.1.23.2
export LD_LIBRARY_PATH=$HOME/.local/lib/python3.10/site-packages/onnxruntime/capi:/usr/local/cuda/lib64:${LD_LIBRARY_PATH:-}
~/Documents/recodee/rust/target/release/codex-gpu-embedder > /tmp/embed.log 2>&1 &
```

The `codex-gpu-embedder` binary lives inside the recodee Rust workspace
(`rust/codex-lb-runtime/...`). It exposes an `/healthz` endpoint at
`http://localhost:8100/healthz` returning the active embedding backend.
Outside recodee there is no embedder to start; the fleet's Colony queries
work fine with whatever embedding provider Colony itself ships with.

## `colony config set embedding.provider codex-gpu` rejected as "invalid enum"

The globally-installed `colony` CLI is older than the workspace build that
added `codex-gpu` to the EmbeddingProvider enum. Reinstall from the
workspace inside recodee:

```bash
cd ~/Documents/recodee/colony
cd packages/config && bun run build && cd -
cd packages/embedding && bun run build && cd -
cd packages/core && bun run build && cd -
cd apps/cli && bun run build && cd -
cd apps/cli && npm install -g .
```

This is a recodee-internal build path. The published `colony` package
(npm / wherever it ends up) will eventually include `codex-gpu` natively
and this step won't be needed.
