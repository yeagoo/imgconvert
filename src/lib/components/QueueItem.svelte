<!-- SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import {
    ArrowsClockwise,
    CheckCircle,
    Image,
    WarningCircle,
    X,
  } from "phosphor-svelte";
  import FormatSelect from "$lib/components/FormatSelect.svelte";
  import {
    extOf,
    formatAccent,
    formatFromExt,
    formatLabel,
    itemProgress,
    itemTargetFormat,
    removeItem,
    setItemTargetFormat,
    settings,
    ui,
    type QueueItem,
  } from "$lib/state.svelte";

  let { item }: { item: QueueItem } = $props();

  const busy = $derived(ui.converting || ui.importing);
  const sourceFormat = $derived(formatFromExt(extOf(item.path)));
  const targetFormat = $derived(itemTargetFormat(item));
  const sourceAccent = $derived(formatAccent(sourceFormat));
  const targetAccent = $derived(formatAccent(targetFormat));
  const progress = $derived(itemProgress(item));
  const sourceFormats = $derived(sourceFormat ? [sourceFormat] : []);
  const statusLabel = $derived(
    item.status === "running"
      ? "转换中"
      : item.status === "done"
        ? "完成"
        : item.status === "skipped"
          ? "跳过"
          : item.status === "error"
            ? "错误"
            : "待转换",
  );

  function updateFormat(value: string) {
    setItemTargetFormat(item.path, value === "__global" ? null : value);
  }
</script>

<li
  class="group grid min-h-40 grid-cols-[88px_minmax(0,1fr)] gap-3 rounded-lg border bg-background p-3 transition-colors ease-[var(--motion-ease-img)] hover:border-primary/35"
>
  <div
    class="flex h-full min-h-32 flex-col justify-between rounded-md border p-2 {sourceAccent.border} {sourceAccent.background}"
  >
    <Image size={24} weight="duotone" class={sourceAccent.text} />
    <div class="space-y-1">
      <div class="text-[11px] font-medium uppercase tracking-wide {sourceAccent.text}">
        {sourceFormat ? formatLabel(sourceFormat) : "IMG"}
      </div>
      <div class="h-1.5 overflow-hidden rounded-full bg-background/70">
        <div
          class="h-full rounded-full bg-primary transition-all duration-300 ease-[var(--motion-ease-img)]"
          style={`width: ${progress}%`}
        ></div>
      </div>
    </div>
  </div>

  <div class="flex min-w-0 flex-col gap-3">
    <div class="flex min-w-0 items-start gap-2">
      <div class="min-w-0 flex-1">
        <div class="truncate text-sm font-medium" title={item.path}>{item.name}</div>
        <div class="truncate text-xs text-muted-foreground" title={item.path}>{item.path}</div>
      </div>

      <button
        class="rounded-md p-1 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive disabled:opacity-30"
        onclick={() => removeItem(item.path)}
        disabled={busy}
        aria-label="移除"
      >
        <X size={16} />
      </button>
    </div>

    <div class="flex flex-wrap items-center gap-2">
      <span
        class="inline-flex h-6 items-center gap-1 rounded-md border px-2 text-xs {targetAccent.border} {targetAccent.background} {targetAccent.text}"
      >
        {#if item.status === "running"}
          <ArrowsClockwise size={13} class="animate-spin" />
        {:else if item.status === "done"}
          <CheckCircle size={13} weight="fill" />
        {:else if item.status === "skipped"}
          <WarningCircle size={13} weight="fill" />
        {:else if item.status === "error"}
          <WarningCircle size={13} weight="fill" />
        {/if}
        {statusLabel}
      </span>

      <FormatSelect
        value={item.targetFormat ?? "__global"}
        includeGlobal
        globalLabel={`跟随 ${formatLabel(settings.format)}`}
        triggerClass="w-36"
        triggerSize="sm"
        disabled={busy}
        {sourceFormats}
        onChange={updateFormat}
      />
    </div>

    <div class="min-h-8">
      {#if item.detail}
        <div
          class="line-clamp-2 text-xs {item.status === 'error' ? 'text-destructive' : 'text-muted-foreground'}"
        >
          {item.detail}
        </div>
      {:else}
        <div class="text-xs text-muted-foreground">
          输出为 {formatLabel(targetFormat)}
        </div>
      {/if}
    </div>
  </div>
</li>
