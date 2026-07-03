<!-- SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import {
    ArrowClockwise,
    CheckCircle,
    FolderOpen,
    PlugsConnected,
    PuzzlePiece,
    Trash,
    WarningCircle,
    X,
  } from "phosphor-svelte";
  import { Button } from "$lib/components/ui/button";
  import {
    loadCodecDiagnostics,
    pickSystemPaths,
    setSelectedHeicHelperPath,
    type CodecDiagnostics,
    type ManifestDiagnostic,
    type ManifestSearchDirDiagnostic,
    type SystemCodecDiagnostic,
    type SystemHelperDiagnostic,
  } from "$lib/state.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();
  let diagnostics = $state<CodecDiagnostics | null>(null);
  let loading = $state(false);
  let helperBusy = $state(false);
  let loadError = $state("");
  let actionMessage = $state("");
  let wasOpen = $state(false);

  const heic = $derived(diagnostics?.heic ?? null);
  const acceptedManifests = $derived(
    heic?.manifestDirs.flatMap((dir) =>
      dir.manifests.filter((manifest) => manifest.status === "accepted"),
    ) ?? [],
  );
  const rejectedManifests = $derived(
    heic?.manifestDirs.flatMap((dir) =>
      dir.manifests.filter((manifest) => manifest.status === "rejected"),
    ) ?? [],
  );
  const availableSystemCodecs = $derived(
    heic?.systemCodecs.filter((codec) => codec.available) ?? [],
  );
  const availableHelpers = $derived(heic?.systemHelpers.filter((helper) => helper.available) ?? []);

  $effect(() => {
    const opened = open && !wasOpen;
    wasOpen = open;
    if (!opened || loading) return;
    void refresh();
  });

  async function refresh() {
    loading = true;
    loadError = "";
    try {
      diagnostics = await loadCodecDiagnostics();
    } catch (error) {
      loadError = `无法读取插件诊断:${String(error)}`;
    } finally {
      loading = false;
    }
  }

  async function chooseSelectedHelper() {
    if (helperBusy) return;
    helperBusy = true;
    actionMessage = "";
    try {
      const selected = await pickSystemPaths({
        multiple: false,
        directory: false,
        title: "选择 HEIC helper",
      });
      if (!selected[0]) return;
      const diagnostic = await setSelectedHeicHelperPath(selected[0]);
      actionMessage = diagnostic.available
        ? "手动 helper 已启用"
        : (diagnostic.message ?? "手动 helper 不可用");
      await refresh();
    } catch (error) {
      actionMessage = `设置手动 helper 失败:${String(error)}`;
    } finally {
      helperBusy = false;
    }
  }

  async function clearSelectedHelper() {
    if (helperBusy) return;
    helperBusy = true;
    actionMessage = "";
    try {
      await setSelectedHeicHelperPath(null);
      actionMessage = "已清除手动 helper";
      await refresh();
    } catch (error) {
      actionMessage = `清除手动 helper 失败:${String(error)}`;
    } finally {
      helperBusy = false;
    }
  }

  function close() {
    open = false;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (open && event.key === "Escape") close();
  }

  function handleBackdropClick(event: MouseEvent) {
    if (event.target === event.currentTarget) close();
  }

  function statusText(status: string): string {
    switch (status) {
      case "accepted":
        return "可用";
      case "ready":
        return "就绪";
      case "rejected":
        return "拒绝";
      case "missing":
        return "缺失";
      case "empty":
        return "空";
      case "untrusted":
        return "不受信任";
      case "unreadable":
        return "不可读";
      case "notDirectory":
        return "非目录";
      case "unsupported":
        return "未启用";
      case "disabled":
        return "已禁用";
      default:
        return status;
    }
  }

  function statusTone(status: string): string {
    if (status === "accepted" || status === "ready") {
      return "border-emerald-500/35 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300";
    }
    if (status === "rejected" || status === "untrusted" || status === "unreadable") {
      return "border-destructive/35 bg-destructive/10 text-destructive";
    }
    return "border-border bg-muted text-muted-foreground";
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div
    class="fixed inset-0 z-50 grid bg-background/80 p-3 backdrop-blur-sm sm:place-items-center"
    role="presentation"
    onclick={handleBackdropClick}
  >
    <div
      class="flex h-full min-h-0 w-full flex-col overflow-hidden rounded-lg border bg-card shadow-xl sm:h-[min(82vh,820px)] sm:max-w-5xl"
      role="dialog"
      aria-modal="true"
      aria-labelledby="plugin-diagnostics-title"
      tabindex="-1"
    >
      <header class="flex items-start gap-3 border-b px-4 py-3">
        <PuzzlePiece size={22} weight="duotone" class="mt-0.5 text-primary" />
        <div class="min-w-0 flex-1">
          <h2 id="plugin-diagnostics-title" class="text-sm font-semibold">插件诊断</h2>
          <p class="mt-1 text-xs text-muted-foreground">
            HEIC 可选导入 provider、manifest 搜索路径与 helper 探测。macOS 使用系统 ImageIO;Windows
            优先使用 WIC;Linux 可用 heif-convert。
          </p>
        </div>
        <Button variant="ghost" size="icon" title="刷新" onclick={refresh} disabled={loading}>
          <ArrowClockwise class={loading ? "animate-spin" : ""} />
        </Button>
        <Button variant="ghost" size="icon" title="关闭" onclick={close}>
          <X />
        </Button>
      </header>

      <div class="min-h-0 flex-1 overflow-auto bg-background px-4 py-4">
        {#if loadError}
          <p
            class="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive"
          >
            {loadError}
          </p>
        {:else if loading && !diagnostics}
          <div class="flex items-center gap-2 text-sm text-muted-foreground">
            <ArrowClockwise size={16} class="animate-spin" />
            正在读取插件诊断…
          </div>
        {:else if heic}
          <div class="grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(280px,360px)]">
            <section class="rounded-lg border bg-card p-3">
              <div class="flex items-start justify-between gap-3">
                <div class="min-w-0">
                  <div class="flex items-center gap-2">
                    {#if heic.enabled}
                      <CheckCircle size={18} weight="fill" class="text-emerald-600" />
                      <h3 class="text-sm font-semibold">HEIC 可选导入已启用</h3>
                    {:else}
                      <WarningCircle size={18} weight="fill" class="text-muted-foreground" />
                      <h3 class="text-sm font-semibold">HEIC 可选导入未启用</h3>
                    {/if}
                  </div>
                  <p class="mt-1 text-xs text-muted-foreground">
                    扩展名: {heic.extensions.join(" / ")}
                  </p>
                  {#if heic.disabledReason}
                    <p class="mt-1 text-xs text-muted-foreground">
                      {heic.disabledReason}
                    </p>
                  {/if}
                </div>
                <span
                  class="rounded-md border px-2 py-1 text-xs {heic.enabled
                    ? 'border-emerald-500/35 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                    : 'bg-muted text-muted-foreground'}"
                >
                  {heic.enabled ? "active" : "inactive"}
                </span>
              </div>

              {#if heic.activeProvider}
                <div class="mt-3 rounded-md border bg-background p-3">
                  <div class="flex items-center gap-2 text-sm font-medium">
                    <PlugsConnected size={16} weight="duotone" class="text-primary" />
                    {heic.activeProvider.id}
                  </div>
                  <dl class="mt-2 grid gap-2 text-xs text-muted-foreground sm:grid-cols-2">
                    <div class="min-w-0">
                      <dt class="font-medium text-foreground">类型</dt>
                      <dd>{heic.activeProvider.kind}</dd>
                    </div>
                    <div class="min-w-0">
                      <dt class="font-medium text-foreground">许可</dt>
                      <dd>
                        {heic.activeProvider.license ??
                          (heic.activeProvider.kind === "system-imageio"
                            ? "系统 ImageIO"
                            : heic.activeProvider.kind === "system-wic"
                              ? "Windows WIC"
                              : "系统 helper")}
                      </dd>
                    </div>
                    <div class="min-w-0 sm:col-span-2">
                      <dt class="font-medium text-foreground">执行文件</dt>
                      <dd class="truncate" title={heic.activeProvider.path}>
                        {heic.activeProvider.path}
                      </dd>
                    </div>
                    <div class="min-w-0 sm:col-span-2">
                      <dt class="font-medium text-foreground">argv</dt>
                      <dd class="truncate font-mono" title={heic.activeProvider.args.join(" ")}>
                        {heic.activeProvider.args.length > 0
                          ? heic.activeProvider.args.join(" ")
                          : "无外部进程参数"}
                      </dd>
                    </div>
                  </dl>
                </div>
              {:else}
                <p
                  class="mt-3 rounded-md border bg-background px-3 py-2 text-sm text-muted-foreground"
                >
                  未发现可用 manifest provider 或系统 helper。
                </p>
              {/if}
            </section>

            <section class="rounded-lg border bg-card p-3">
              <h3 class="text-sm font-semibold">探测摘要</h3>
              <dl class="mt-3 grid grid-cols-2 gap-2 text-xs">
                {@render SummaryStat("外部 codec", heic.externalCodecsEnabled ? "启用" : "禁用")}
                {@render SummaryStat(
                  "手动 helper",
                  heic.selectedHelper.available
                    ? "可用"
                    : heic.selectedHelper.configured
                      ? "不可用"
                      : "未配置",
                )}
                {@render SummaryStat("可用 manifest", acceptedManifests.length)}
                {@render SummaryStat("拒绝 manifest", rejectedManifests.length)}
                {@render SummaryStat("系统 codec", availableSystemCodecs.length)}
                {@render SummaryStat("系统 helper", availableHelpers.length)}
              </dl>
            </section>
          </div>

          <section class="mt-3 rounded-lg border bg-card p-3">
            <h3 class="text-sm font-semibold">许可与渠道边界</h3>
            <p class="mt-2 text-xs leading-5 text-muted-foreground">
              主程序不内置 HEIC codec,不链接 libheif,不分发 x265。HEIC 仅通过用户环境中的可选
              decode-only helper/provider 导入;插件许可、源码/NOTICE
              与专利风险需由插件分发方单独处理。商店或 Flatpak 构建可通过禁用外部 codec
              自动发现保持主包边界。Windows 优先探测系统 WIC HEIF/HEVC
              扩展;缺失时可按诊断提示安装系统扩展, 或使用单独 helper,不把 HEIC codec 打进主程序。
            </p>
          </section>

          <section class="mt-3 rounded-lg border bg-card p-3">
            <h3 class="text-sm font-semibold">系统 Codec</h3>
            <div class="mt-2 grid gap-2 md:grid-cols-2">
              {#each heic.systemCodecs as codec (codec.id)}
                {@render SystemCodecRow(codec)}
              {:else}
                <p class="text-sm text-muted-foreground">当前平台没有系统 codec 探测项。</p>
              {/each}
            </div>
          </section>

          <section class="mt-3 rounded-lg border bg-card p-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <h3 class="text-sm font-semibold">手动 Helper</h3>
              <div class="flex items-center gap-1">
                <Button
                  variant="outline"
                  size="sm"
                  onclick={chooseSelectedHelper}
                  disabled={helperBusy || !heic.externalCodecsEnabled}
                >
                  <FolderOpen />
                  选择
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onclick={clearSelectedHelper}
                  disabled={helperBusy || !heic.selectedHelper.configured}
                >
                  <Trash />
                  清除
                </Button>
              </div>
            </div>
            {#if actionMessage}
              <p class="mt-2 text-xs text-muted-foreground">{actionMessage}</p>
            {/if}
            {#if heic.selectedHelper.configured}
              <div class="mt-3 rounded-md border bg-background p-3">
                <div class="flex items-center justify-between gap-2">
                  <span class="text-xs font-medium">
                    {heic.selectedHelper.available ? "可用" : "不可用"}
                  </span>
                  <span
                    class="rounded border px-1.5 py-0.5 text-[11px] {heic.selectedHelper.available
                      ? 'border-emerald-500/35 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                      : 'bg-muted text-muted-foreground'}"
                  >
                    selected
                  </span>
                </div>
                <p
                  class="mt-2 truncate font-mono text-xs text-muted-foreground"
                  title={heic.selectedHelper.path ?? ""}
                >
                  {heic.selectedHelper.path}
                </p>
                {#if heic.selectedHelper.message}
                  <p class="mt-2 break-words font-mono text-[11px] leading-4 text-destructive">
                    {heic.selectedHelper.message}
                  </p>
                {/if}
              </div>
            {:else}
              <p
                class="mt-3 rounded-md border bg-background px-3 py-2 text-sm text-muted-foreground"
              >
                未配置手动 helper。
              </p>
            {/if}
          </section>

          <section class="mt-3 rounded-lg border bg-card p-3">
            <h3 class="text-sm font-semibold">系统 Helper</h3>
            <div class="mt-2 grid gap-2 md:grid-cols-2">
              {#each heic.systemHelpers as helper (helper.command)}
                {@render HelperRow(helper)}
              {:else}
                <p class="text-sm text-muted-foreground">网页预览环境不探测本机 helper。</p>
              {/each}
            </div>
          </section>

          <section class="mt-3 rounded-lg border bg-card p-3">
            <h3 class="text-sm font-semibold">Manifest 搜索</h3>
            <div class="mt-2 space-y-2">
              {#each heic.manifestDirs as dir (`${dir.source}:${dir.path}`)}
                {@render ManifestDir(dir)}
              {:else}
                <p class="text-sm text-muted-foreground">网页预览环境不探测 manifest。</p>
              {/each}
            </div>
          </section>
        {/if}
      </div>
    </div>
  </div>
{/if}

{#snippet SummaryStat(label: string, value: number | string)}
  <div class="rounded-md border bg-background px-3 py-2">
    <dt class="text-muted-foreground">{label}</dt>
    <dd class="mt-1 text-lg font-semibold tabular-nums text-foreground">{value}</dd>
  </div>
{/snippet}

{#snippet SystemCodecRow(codec: SystemCodecDiagnostic)}
  <div class="rounded-md border bg-background p-3">
    <div class="flex items-center justify-between gap-2">
      <span class="font-mono text-xs font-medium">{codec.id}</span>
      <span
        class="rounded border px-1.5 py-0.5 text-[11px] {codec.available
          ? 'border-emerald-500/35 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
          : 'bg-muted text-muted-foreground'}"
      >
        {codec.available ? "available" : "missing"}
      </span>
    </div>
    <p class="mt-2 text-xs text-muted-foreground">{codec.message}</p>
    {#if codec.installHint}
      <p class="mt-2 text-xs text-muted-foreground">{codec.installHint}</p>
    {/if}
    <p class="mt-2 font-mono text-[11px] text-muted-foreground">
      {codec.kind} · {codec.readable.join(" / ")}
    </p>
  </div>
{/snippet}

{#snippet HelperRow(helper: SystemHelperDiagnostic)}
  <div class="rounded-md border bg-background p-3">
    <div class="flex items-center justify-between gap-2">
      <span class="font-mono text-xs font-medium">{helper.command}</span>
      <span
        class="rounded border px-1.5 py-0.5 text-[11px] {helper.available
          ? 'border-emerald-500/35 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
          : 'bg-muted text-muted-foreground'}"
      >
        {helper.available ? "found" : "missing"}
      </span>
    </div>
    <p
      class="mt-2 truncate text-xs text-muted-foreground"
      title={helper.path ?? helper.message ?? ""}
    >
      {helper.path ?? helper.message}
    </p>
  </div>
{/snippet}

{#snippet ManifestDir(dir: ManifestSearchDirDiagnostic)}
  <div class="rounded-md border bg-background p-3">
    <div class="flex flex-wrap items-start justify-between gap-2">
      <div class="min-w-0">
        <div class="flex items-center gap-2">
          <span class="rounded border px-1.5 py-0.5 text-[11px] text-muted-foreground">
            {dir.source}
          </span>
          <span class="rounded border px-1.5 py-0.5 text-[11px] {statusTone(dir.status)}">
            {statusText(dir.status)}
          </span>
        </div>
        <p class="mt-2 truncate font-mono text-xs text-muted-foreground" title={dir.path}>
          {dir.path}
        </p>
      </div>
    </div>
    {#if dir.message}
      <p class="mt-2 text-xs text-muted-foreground">{dir.message}</p>
    {/if}

    {#if dir.manifests.length}
      <div class="mt-3 space-y-2">
        {#each dir.manifests as manifest (manifest.path)}
          {@render ManifestRow(manifest)}
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

{#snippet ManifestRow(manifest: ManifestDiagnostic)}
  <div class="rounded-md border bg-card px-3 py-2">
    <div class="flex items-center justify-between gap-2">
      <p class="min-w-0 truncate font-mono text-xs" title={manifest.path}>
        {manifest.path}
      </p>
      <span class="shrink-0 rounded border px-1.5 py-0.5 text-[11px] {statusTone(manifest.status)}">
        {statusText(manifest.status)}
      </span>
    </div>
    {#if manifest.provider}
      <p class="mt-1 text-xs text-muted-foreground">
        {manifest.provider.id} · {manifest.provider.license ?? "无许可声明"}
      </p>
    {/if}
    {#if manifest.message}
      <p class="mt-1 break-words font-mono text-[11px] leading-4 text-destructive">
        {manifest.message}
      </p>
    {/if}
  </div>
{/snippet}
