---
title: The view of a batch
slug: /operator-help/screens/batch
sidebar_position: 18
---

# The view of a batch

> One batch: where it stands, what you asked, what it produced, item by item.

## What it is for

The view of a batch opens once a batch is launched, and again from the history of the batches. It says where the batch stands, recalls what you asked the agent, lists the files the batch produced, and shows its results: as a table when you asked for result columns, and item by item in every case.

## The header bar

The bar at the top follows the batch while it runs:

- Progress: how many items are processed out of the total, with the count of items succeeded, failed, waiting for approval and still to process.
- Pace: the time taken per item and the time the batch should end around.
- **Pause batch**, **Resume** and **Stop**: a paused batch starts no new item until you resume it; the item in progress ends. A stopped batch does not start again, and its results can still be exported.
- **Retry failures**: runs the failed items again.
- **Export…**: writes the results as an Excel workbook (.xlsx), a CSV file (.csv) or a JSON document (.json). Each field of the answer is its own column. For a folder batch, the export holds the name of each file, not its text.

## What you asked, and the files produced

The card **What you asked** recalls the launch: the instruction sent with every item, the result columns with what each one holds, and the synthesis asked at the end, if any.

Beside it, **Files produced** lists what the batch wrote on this computer:

- the workbook with the agent's columns, for a batch on an Excel or CSV file;
- the synthesis of the batch, when one was asked;
- the exports made from this screen.

Each file offers **Open** and **Show in folder**.

When a synthesis was asked, the card **Batch synthesis** shows it once the last item has run.

## Results and Items

Once a batch with result columns has completed items, two tabs appear: **Results** and **Items**. While the batch runs, the view opens on **Items**; once it has ended, on **Results**.

**Results** is a table: one row per item, one column per field.

- Click the header of a column to sort it. Amounts are read as numbers.
- A field that takes few values becomes a filter, with the count of each value, for example "Compliant: yes 36, no 4". **Show all** removes the filters.
- A cell left empty, or an answer that did not give its fields, is marked to read again.
- Click a row to open the item.

**Items** shows the list of items beside the detail of the one you open: its steps, its answer field by field, its input and the sources it consulted. Above the list, tabs sort the items by state, such as **All**, **Succeeded** or **Failed**, and a search box filters them. While the batch runs, **Follow the running item** keeps the detail on the item being processed.

An item that waits for an access not granted for the batch also waits in **To review**: see [Approve or reject an agent action](/operator-help/control/approve-or-reject-an-action).

## Continue in the Chat

Once the batch has ended, **Continue in the Chat** opens a conversation with the agent of the batch. The table of results is joined to it as a CSV file, and the start of a request is written in the composer. Nothing is sent: finish the request, then send it.

## Run the batch again on another source

Once the batch has ended, **Run this batch again…** opens the launch window with the same agent, the same instruction, the same result columns and the same synthesis. Only the source is left to choose.

## What you can do

- Follow the batch, pause it, resume it or stop it.
- Read the results as a table, sort and filter them, and open an item to read its detail.
- Open the files the batch produced, or export its results.
- Run its failures again.
- Continue in the Chat with the table of results, or run the same batch on another source.

## Go further

- [Hand a task to an agent](/operator-help/agents/start-an-agent)
- [The history of the batches](/operator-help/screens/batches)
- [Approve or reject an agent action](/operator-help/control/approve-or-reject-an-action)
