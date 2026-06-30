<!-- SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { UploadSimple } from "phosphor-svelte";
  import { Button } from "$lib/components/ui/button";
  import { addPaths, isTauriRuntime, ui, readableExtensions } from "$lib/state.svelte";

  let previewMessage = $state("");

  async function pick() {
    if (ui.converting) return;

    if (!isTauriRuntime()) {
      previewMessage = "网页预览无法读取本机文件路径,请在 Tauri 桌面端选择文件";
      return;
    }

    const sel = await open({
      multiple: true,
      filters: [{ name: "图片", extensions: readableExtensions() }],
    });
    if (Array.isArray(sel)) addPaths(sel);
    else if (typeof sel === "string") addPaths([sel]);
  }
</script>

<section
  class="rounded-lg border-2 border-dashed bg-card p-6 text-center transition-colors ease-[var(--motion-ease-img)]
         {ui.dragActive ? 'border-primary bg-primary/5' : 'border-border'}"
>
  <UploadSimple size={34} weight="duotone" class="mx-auto text-muted-foreground" />
  <p class="mt-2 text-sm font-medium">拖拽图片到这里</p>
  <p class="mb-3 text-xs text-muted-foreground">支持批量 · 或</p>
  <Button variant="outline" size="sm" onclick={pick} disabled={ui.converting}>选择文件…</Button>
  {#if previewMessage}
    <p class="mt-3 text-xs text-muted-foreground">{previewMessage}</p>
  {/if}
</section>
