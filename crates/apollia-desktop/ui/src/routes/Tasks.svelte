<script lang="ts">
  import {
    Card,
    CardHeader,
    CardTitle,
    CardDescription,
    CardContent,
  } from "$lib/components/ui/card";
</script>

<div class="space-y-4">
  <h1 class="text-2xl font-bold">Tasks</h1>
  <Card>
    <CardHeader>
      <CardTitle>Task Monitor</CardTitle>
      <CardDescription>Track submitted tasks and their progress.</CardDescription>
    </CardHeader>
    <CardContent>
      <p class="text-sm text-muted-foreground">Task list will appear here.</p>
    </CardContent>
  </Card>
</div>
