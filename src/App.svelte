<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright (C) 2026 ImgConvert contributors -->
<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import {
    IconContext,
    ArrowsClockwise,
    CheckCircle,
    PlayCircle,
    StopCircle,
    Trash,
    WarningCircle,
  } from "phosphor-svelte";
  import { Button } from "$lib/components/ui/button";
  import Dropzone from "$lib/components/Dropzone.svelte";
  import QueueItem from "$lib/components/QueueItem.svelte";
  import SettingsBar from "$lib/components/SettingsBar.svelte";
  import Topbar from "$lib/components/Topbar.svelte";
  import {
    addDemoItems,
    addPaths,
    cancelConversion,
    checkEngine,
    clearQueue,
    convertAll,
    engine,
    initPersistence,
    isTauriRuntime,
    queue,
    ui,
  } from "$lib/state.svelte";

  const doneCount = $derived(queue.filter((item) => item.status === "done").length);
  const skippedCount = $derived(queue.filter((item) => item.status === "skipped").length);
  const errorCount = $derived(queue.filter((item) => item.status === "error").length);
  const runningCount = $derived(queue.filter((item) => item.status === "running").length);
  const settledCount = $derived(doneCount + skippedCount + errorCount);
  const overallProgress = $derived(
    queue.length ? Math.round((settledCount / queue.length) * 100) : 0,
  );
  const progressLabel = $derived(
    queue.length
      ? `${doneCount}/${queue.length} 完成${skippedCount ? ` · ${skippedCount} 跳过` : ""}${errorCount ? ` · ${errorCount} 错误` : ""}`
      : "0 个文件",
  );

  onMount(() => {
    const runningInTauri = isTauriRuntime();
    void initPersistence().then(async () => {
      await checkEngine();
      if (!runningInTauri) addDemoItems();
    });

    if (!runningInTauri) return;

    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (ui.converting) {
        ui.dragActive = false;
        return;
      }

      if (event.payload.type === "over") {
        ui.dragActive = true;
      } else if (event.payload.type === "drop") {
        ui.dragActive = false;
        addPaths(event.payload.paths);
      } else {
        ui.dragActive = false;
      }
    });

    return () => {
      unlisten.then((cleanup) => cleanup());
    };
  });
</script>

<IconContext values={{ weight: "duotone", size: 20 }}>
  <main class="relative flex h-screen flex-col gap-4 overflow-hidden bg-background p-4 text-foreground">
    {#if ui.dragActive}
      <div
        class="pointer-events-none fixed inset-0 z-40 grid place-items-center border-4 border-primary/50 bg-primary/10 backdrop-blur-sm"
      >
        <div class="rounded-lg border bg-card px-4 py-2 text-sm font-medium shadow-md">
          释放以添加图片
        </div>
      </div>
    {/if}

    <Topbar />
    <Dropzone />
    <SettingsBar />

    <section class="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden">
      <div class="rounded-md border bg-card px-3 py-2">
        <div class="flex items-center justify-between gap-3">
          <div class="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
            {#if runningCount}
              <ArrowsClockwise size={15} class="animate-spin text-primary" />
            {:else if errorCount}
              <WarningCircle size={15} weight="fill" class="text-destructive" />
            {:else if settledCount && settledCount === queue.length && !skippedCount}
              <CheckCircle size={15} weight="fill" class="text-emerald-600" />
            {:else if skippedCount}
              <WarningCircle size={15} weight="fill" class="text-muted-foreground" />
            {/if}
            <span class="truncate">{progressLabel}</span>
          </div>

          <div class="flex items-center gap-1">
            <Button
              variant={ui.converting ? "destructive" : "default"}
              size="sm"
              onclick={ui.converting ? cancelConversion : convertAll}
              disabled={
                ui.converting
                  ? ui.cancelRequested
                  : !queue.length || !engine.ok
              }
            >
              {#if ui.converting}
                <StopCircle />
                {ui.cancelRequested ? "取消中" : "取消"}
              {:else}
                <PlayCircle />
                全部转换
              {/if}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onclick={clearQueue}
              disabled={ui.converting || !queue.length}
            >
              <Trash />
              清空
            </Button>
          </div>
        </div>

        {#if queue.length}
          <div class="mt-2 h-1.5 overflow-hidden rounded-full bg-muted">
            <div
              class="h-full rounded-full bg-primary transition-all duration-300 ease-[var(--motion-ease-img)]"
              style={`width: ${overallProgress}%`}
            ></div>
          </div>
        {/if}
      </div>

      <ul
        class="grid min-h-0 flex-1 grid-cols-1 content-start gap-3 overflow-y-auto md:grid-cols-2 xl:grid-cols-3"
      >
        {#each queue as item (item.path)}
          <QueueItem {item} />
        {:else}
          <li class="col-span-full flex h-full min-h-48 items-center justify-center rounded-lg border border-dashed p-8 text-sm text-muted-foreground">
            还没有文件
          </li>
        {/each}
      </ul>
    </section>
  </main>
</IconContext>
