<!-- SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { FolderOpen, Images, StopCircle, UploadSimple } from "phosphor-svelte";
  import { Button } from "$lib/components/ui/button";
  import {
    cancelImportScan,
    importPaths,
    isTauriRuntime,
    ui,
    readableExtensions,
  } from "$lib/state.svelte";

  const busy = $derived(ui.converting || ui.importing);
  const importErrorPreview = $derived(ui.importErrors.slice(0, 5));
  const hiddenImportErrors = $derived(
    Math.max(0, ui.importErrors.length - importErrorPreview.length),
  );

  async function pickFiles() {
    if (busy) return;

    if (!isTauriRuntime()) {
      ui.importMessage = "网页预览无法读取本机文件路径,请在 Tauri 桌面端选择文件";
      return;
    }

    const sel = await open({
      multiple: true,
      filters: [{ name: "图片", extensions: readableExtensions() }],
    });
    if (busy) return;
    if (Array.isArray(sel)) await importPaths(sel);
    else if (typeof sel === "string") await importPaths([sel]);
  }

  async function pickDirectories() {
    if (busy) return;

    if (!isTauriRuntime()) {
      ui.importMessage = "网页预览无法读取本机目录,请在 Tauri 桌面端选择文件夹";
      return;
    }

    const sel = await open({
      directory: true,
      multiple: true,
    });
    if (busy) return;
    if (Array.isArray(sel)) await importPaths(sel);
    else if (typeof sel === "string") await importPaths([sel]);
  }
</script>

<section
  class="rounded-lg border-2 border-dashed bg-card p-6 text-center transition-colors ease-[var(--motion-ease-img)]
         {ui.dragActive ? 'border-primary bg-primary/5' : 'border-border'}"
>
  <UploadSimple size={34} weight="duotone" class="mx-auto text-muted-foreground" />
  <p class="mt-2 text-sm font-medium">
    {ui.importing ? "正在扫描图片…" : "拖拽图片或文件夹到这里"}
  </p>
  <p class="mb-3 text-xs text-muted-foreground">支持批量 · 递归目录 · 自动去重</p>
  <div class="flex flex-wrap justify-center gap-2">
    <Button variant="outline" size="sm" onclick={pickFiles} disabled={busy}>
      <Images weight="duotone" />
      选择文件
    </Button>
    <Button variant="outline" size="sm" onclick={pickDirectories} disabled={busy}>
      <FolderOpen weight="duotone" />
      选择文件夹
    </Button>
    {#if ui.importing}
      <Button
        variant="ghost"
        size="sm"
        onclick={cancelImportScan}
        disabled={ui.importCancelRequested}
      >
        <StopCircle weight="duotone" />
        {ui.importCancelRequested ? "取消中" : "取消扫描"}
      </Button>
    {/if}
  </div>
  {#if ui.importMessage}
    <p class="mt-3 text-xs text-muted-foreground">{ui.importMessage}</p>
  {/if}
  {#if ui.importErrors.length}
    <details class="mx-auto mt-3 max-w-3xl text-left text-xs text-muted-foreground">
      <summary class="cursor-pointer text-center hover:text-foreground">
        查看导入错误
      </summary>
      <ul class="mt-2 space-y-1 rounded-md border bg-background p-2">
        {#each importErrorPreview as error}
          <li class="min-w-0">
            <span class="font-medium text-foreground">{error.message}</span>
            <span class="block truncate" title={error.path}>{error.path}</span>
          </li>
        {/each}
        {#if hiddenImportErrors}
          <li>还有 {hiddenImportErrors} 个错误未显示</li>
        {/if}
      </ul>
    </details>
  {/if}
</section>
