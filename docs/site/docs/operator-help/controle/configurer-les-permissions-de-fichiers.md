# Manage tool permissions

> For operators who want to review, filter or revoke the permissions granted to an agent's tools - without waiting for an action to raise a new approval card.

## Prerequisites

- The application is open and at least one agent has been run.
- Permissions were granted during an earlier session (during an approval, you chose "Always allow" or a persisted scope).

## How are permissions created?

Rules appear automatically when an agent requests access to a tool and you choose a persisted scope in the approval card (for example: *This project* or *Everywhere*). Chat rules (*Chat* scope) are created through the **"Always allow"** button in the free chat. You can also create one by hand, with the **Add rule** button at the top of the Rules section, which is how you authorise something before an agent ever asks for it.

## Review the active permissions

1. In the sidebar, click **Settings**.

2. In the left menu, select **Permissions**.
   ![Settings > Permissions page, list of permission cards (PermissionRuleCard) with scope badges](/img/operator-help/en/controle-configurer-les-permissions-de-fichiers-1.png)

3. The central panel shows every active rule as a **list of cards**. Each card states:
   - the **tool name** that is authorised (e.g.: `bash_executor`, `file_write`, `http_fetch`)
   - a **scope badge**: *This project* or *Everywhere*
   - the **argument prefix** (if the rule is limited to certain invocations)
   - the **expiry date** or the mention *Permanent*
   - the **author** of the decision (agent or user)

<!-- claim:prefix-rules-evaluated-per-invocation -->
> **What a rule actually does:** a rule with no prefix that allows an ordinary tool auto-approves every invocation of that tool. A rule carrying an **argument prefix** is evaluated on every invocation, against the call's argument: it auto-approves any argument starting with the prefix, and the longest matching prefix wins when several rules apply. A **deny** rule always takes precedence: it refuses a matching call even when the tool is otherwise covered by an "Always allow".
<!-- claim:executor-guard-blocks-command-chaining -->
> **Code executors** (`bash_executor`, `python_executor`) are stricter: a prefix rule only applies to a **single simple command** sharing that prefix, with no chaining (`;`, `&&`, `||`), pipe, redirection (`>`, `<`) or substitution (`` ` ``, `$(...)`). A rule without a prefix never auto-approves a code executor: every invocation asks for confirmation again.

4. Use the filters in the left panel to narrow the list:
   - **Scope**: *All*, *This project*, *Chat / agent*, *Everywhere*
   - **Tool**: pick a specific tool from the list of tools present

## Revoke a single permission

1. Find the card matching the rule you want to remove.
2. Click the **Revoke** button (bin icon, on the right of the card).
3. A confirmation message appears briefly. The card disappears immediately.
   ![permission card with the Revoke button visible, confirmation toast "Rule bash revoked"](/img/operator-help/en/controle-configurer-les-permissions-de-fichiers-2.png)

Once revoked, the tool involved will ask for manual approval again at its next invocation.

## Active sessions

The **Active sessions** section lists the tools auto-approved through "For this session" in the ongoing chat conversations. These permissions are **in-memory only** - they disappear when the session closes and are not persisted.

Each entry states the tool name, the session involved (title or short identifier), the mode (*Apollia Chat*, *Agent*, *Companion*) and an orange *Session* badge. Click **Revoke** to remove the permission immediately. The tool will ask for confirmation again at the next call in that session.

![Active sessions section, list of entries with an orange Session badge and a Revoke button](/img/operator-help/en/controle-configurer-les-permissions-de-fichiers-3.png)

## Revoke every permission at once

1. Click the red **Revoke all** button at the top right.
2. Choose the scope to purge:
   - *This project* - removes the rules tied to the current project
   - *Chat / agent* - removes the rules tied to the Apollia Chat agent and to Python agents
   - *Everywhere* - removes the global rules
   - *All scopes* - removes every persisted rule
3. Check the number of affected rules shown in the dialog, then click **Revoke**.
   ![Revoke all dialog: the scope selector and the revoke button](/img/operator-help/en/controle-configurer-les-permissions-de-fichiers-1bis.png)

## Chat rules (Apollia Chat)

The **Apollia Chat** section lists the tools auto-approved for every free chat session. These rules are created through **"Always allow"** in the chat and persist from one session to the next. Revoke them one by one here so the tool asks for confirmation again at the next invocation from the chat. Code executors (`bash_executor`, `python_executor`) never show up there: they cannot be auto-approved in bulk and always go back through a per-invocation confirmation.

## Review the recent audit

At the bottom of the page, the **Recent audit** section (read-only) lists the last 20 permission decisions: tool, decision (allow / deny), scope, number of the rule applied and agent involved. This lets you check that a rule is properly applied without having to run an agent.

## If it does not work

- **No permission shown**: no persisted rule exists for the selected filters. Reset the filters, or run an agent and grant a persisted permission through the approval card.
- **The rule comes back after revocation**: another agent (or a global configuration) creates the same rule automatically. Check your agents or contact support.
- **The "Revoke all" button is greyed out**: the list is empty - there is nothing to revoke for the current filters.

> **Technical reference:** [Apollia reference](/reference)
