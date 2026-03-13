<script lang="ts">
  import {
    Card,
    CardHeader,
    CardTitle,
    CardDescription,
    CardContent,
  } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
</script>

<div class="space-y-4">
  <div class="flex items-center gap-3">
    <h1 class="text-2xl font-bold">Approvals</h1>
    <Badge variant="destructive">0 pending</Badge>
  </div>
  <Card>
    <CardHeader>
      <CardTitle>Human-in-the-Loop</CardTitle>
      <CardDescription>Review and approve or reject pending agent actions.</CardDescription>
    </CardHeader>
    <CardContent>
      <p class="text-sm text-muted-foreground">Pending approvals will appear here.</p>
    </CardContent>
  </Card>
</div>
