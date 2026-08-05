# Monitor AI costs

> For operators who want to track what the calls to their AI provider cost over the past week.

## Prerequisites

- At least one conversation or agent has already called your AI provider.
- You use a billed provider (Anthropic, OpenAI, Bedrock, Vertex…). Local models do not appear in the costs.

## Steps

1. In the sidebar, click **Observability**, then the **LLM Costs** tab.

2. At the top right of the card, a **period selector** lets you switch between **7 d / 14 d / 30 d / 90 d / 1 yr**. Every indicator, the chart and the legend recompute instantly over the new window. The density of the horizontal axis adapts automatically (labels thinned out beyond 14 days).

3. At the top, **four key indicators** (KPI) summarise the selected window:
   - **7-day total** - the label is fixed and says seven days whatever period you select, so read it as the sum over the window rather than over a week.
   - **Avg / day** - total divided by the number of days in the window.
   - **Peak day** - amount + date of the day that consumed the most.
   - **Top backend** - name of the backend that weighs the most, with its cumulative total.
   ![LLM Costs tab, with the period selector, the four KPIs, the stacked bar chart and the backend legend](/img/operator-help/observabilite-surveiller-les-couts-llm-1.png)

4. In the centre, a **stacked bar chart** shows the selected period. One bar per day, each bar split into coloured segments by **backend** (Anthropic, OpenAI, and so on). The vertical axis is in dollars with rounded ticks; the horizontal axis shows the date (day of the week + short date for short windows, date only for 30 d and above).

5. Hover a column: the other days fade slightly and the **day total** appears above the bar. A tooltip also appears on each segment with the **backend name** and its **exact cost** (for example `anthropic: $0.42 - May 11`).

6. Below the chart, the **legend** lists every active backend as **pills** showing the **cumulative total per backend** over the window. Handy to compare each provider's share at a glance.

## The alert threshold, and where to read it

The chart says what you spent. It does not say what you decided to allow. That ceiling is `cost_alert_threshold_usd`, under the `[llm]` section of `apollia.toml`, and it is shown in **Builder** mode only, on the **LLM models** page of the sidebar, at the top of the **Session Statistics (7 days)** block.

What you see there:

- **No threshold configured**: one line saying so, and naming the setting to add. That is the default state, no ceiling is set out of the box.
- **A threshold configured**: a **Cost alert threshold** strip with a *spend of ceiling* figure, a gauge, and a caption naming the day being judged. The strip turns amber past 80 % of the ceiling (*Close to threshold*) and red above it (*Over threshold*).

Read the comparison for what it is. The runtime applies this threshold to the **accumulated cost of one session**, while this surface only knows daily totals. So the strip compares the **busiest single day** of the last 7 days against the ceiling, which is an upper bound: a day below the ceiling cannot be hiding a session above it. The caption names that day explicitly.

It is an alert, not a cap. Crossing it interrupts nothing: no call is refused, no run is stopped. The runtime flags the crossing on the session budget it publishes, and this strip shows it on the cost side.

## Verification

The figures at the bottom match your intuition about consumption. The data refreshes automatically about once a minute.

> **Note - hybrid routing:** if you use hybrid routing (`[llm.routing.hybrid]`), the steps escalated to the frontier model appear under the frontier backend in the chart and in the legend. Watch that backend to keep your real consumption in check against the configured `cost_ceiling_usd` ceiling.

## If it does not work

- **The chart is empty**: no billed call was recorded over 7 days. Check that your provider is not a 100 % local model.
- **The costs look too high**: open the **Logs** of the busiest assistant (**My Assistants** page) and look at the longest tasks - a bulky injected context inflates input tokens fast.
- **Costs went up after enabling hybrid routing**: the frontier is being called more often than expected. Lower `cost_ceiling_usd` in `[llm.routing.hybrid]` to limit escalation, or disable hybrid routing temporarily. See [Connect a remote model](../installation/connecter-un-modele-distant.md).

> **Technical reference:** [Apollia reference](/reference)
