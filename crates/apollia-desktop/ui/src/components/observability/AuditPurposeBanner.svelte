<script lang="ts">
  // Purpose banner for the audit trail, explains why the audit trail matters
  // with premium framing. Extracted verbatim from AuditTrailTable so the table
  // stays focused on data orchestration. Stateless presentation.
  import { t } from "svelte-i18n";
  import { Shield } from "lucide-svelte";
</script>

<aside
  class="glass-card glass-border rounded-xl px-5 py-4 flex items-start gap-3"
  data-testid="audit-purpose"
>
  <div class="rounded-lg glass-inset p-2 flex-shrink-0 mt-0.5">
    <Shield class="h-4 w-4 text-primary" aria-hidden="true" />
  </div>
  <div class="min-w-0">
    <h3 class="m-0 text-[13px] font-semibold tracking-[-0.1px] mb-1">
      {$t('observability.audit_purpose_title')}
    </h3>
    <p class="m-0 text-[12.5px] leading-[1.55] text-muted-foreground max-w-[720px]">
      {$t('observability.audit_purpose_body')}
    </p>
  </div>
</aside>
