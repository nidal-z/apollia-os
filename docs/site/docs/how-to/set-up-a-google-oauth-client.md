---
sidebar_position: 6.6
title: Set up a Google OAuth client
---

# Set up a Google OAuth client

Apollia's Google connector needs an OAuth client that belongs to you. This guide
creates one from nothing, names every screen and every button, and ends with
Gmail, Calendar and Drive reachable from Apollia.

Budget ten to fifteen minutes. Nothing here costs money, and none of it requires
a Google Workspace subscription: a personal `@gmail.com` account is enough.

If you only want the short version, the same steps are condensed on
[Connect Google Workspace](/operator-help/integrations/connect-google-workspace).

:::note Microsoft 365 needs none of this
Microsoft connects with nothing to configure, because Apollia ships the
identifier of its own registered application. Google does not allow the
equivalent. [Why the two differ](#why-google-asks-for-this-and-microsoft-does-not)
is at the end of this page.
:::

## Before you start

- A Google account. Personal or Workspace, either works.
- Apollia installed and running.
- A browser signed in to that Google account.

You will move between two windows: the **Google Cloud console** in your browser,
and **Apollia**. The last section is the only one inside Apollia.

## Step 1. Create a Google Cloud project

A project is a container for the client you are about to create. It is free, and
it exists only so Google has somewhere to attach the client.

1. Open [console.cloud.google.com](https://console.cloud.google.com) and sign in.
2. If this is your first visit, accept the terms of service. You may be asked for
   a country and whether you want email updates. No credit card is required.
3. In the blue bar at the top, click the **project selector**, the dropdown just
   to the right of the "Google Cloud" logo. It reads *Select a project*, or shows
   the name of a project you already own.
4. In the dialog that opens, click **New project** at the top right.
5. Under **Project name**, type something you will recognise later, for example
   `Apollia`. Leave **Location** as it is.
6. Click **Create**. A notification appears after a few seconds.
7. Open the project selector again and click your new project, so the blue bar
   shows its name. Everything below applies to the selected project, and picking
   the wrong one here is the most common way to end up confused three steps
   later.

## Step 2. Enable the APIs you want to use

A fresh project can call nothing. You switch on one API per Google service you
want Apollia to reach.

1. In the search bar at the top, type `Gmail API` and pick the **Gmail API**
   result under *Marketplace*.
2. Click **Enable**. Wait for the page to turn into the API's dashboard.
3. Repeat for each service you want:
   - **Google Calendar API** for calendar events.
   - **Google Drive API** for Drive files.
   - **Google Sheets API**, **Google Docs API**, **Google Slides API**,
     **Google Forms API**, **Google Tasks API**, **YouTube Data API v3** if you
     want those too.

Enabling an API you never use costs nothing. Forgetting one shows up later as a
permission error when an agent calls that service, so it is easier to enable
Gmail, Calendar and Drive now.

## Step 3. Configure the consent screen

The consent screen is what you will see when you connect your account: the page
listing what Apollia is asking for. Google requires it to exist before it will
issue a client.

1. In the left-hand navigation, open **APIs & Services**, then **OAuth consent
   screen**. If the menu is hidden, click the hamburger icon at the top left.
2. Google now opens a short branding form. Fill in:
   - **App name**: `Apollia`, or any name you like. This is the name you will see
     on the consent page.
   - **User support email**: pick your own address from the dropdown.
   - **Developer contact information**: your address again, at the bottom of the
     form.
3. For **Audience**, choose **External**. *Internal* is offered only on a
   Workspace account and restricts the client to your organization; **External**
   is the right answer for a personal account and works fine for a Workspace one.
4. Click **Create** (or **Save and continue** through the remaining sections,
   depending on which version of the console you get). You can leave the scopes
   section empty: Apollia asks for what it needs at connection time.

### Add yourself as a test user

A brand new consent screen is in **Testing** status, which means only addresses
you list explicitly may use it.

1. Still under **OAuth consent screen**, open the **Audience** section.
2. Under **Test users**, click **Add users**.
3. Type the Google address you intend to connect to Apollia, press Enter, then
   click **Save**.

Miss this and the connection fails at the consent page with a message about the
app not having completed verification.

## Step 4. Create the OAuth client

1. In the left-hand navigation, under **APIs & Services**, open **Credentials**.
2. Click **+ Create credentials** at the top, then **OAuth client ID**.
3. Under **Application type**, choose **Desktop app**. This is the important
   one: it is the type that permits the loopback redirect Apollia listens on. A
   *Web application* client will be rejected later.
4. Under **Name**, type `Apollia desktop` or anything else. This name is internal
   to the console and is never shown to you again.
5. Click **Create**.

A dialog appears with your **Client ID** and **Client secret**.

6. Click **Download JSON** and keep the file somewhere you can find it in a
   minute. This is the fastest path into Apollia, which reads both values
   straight out of it.

If you close the dialog too early, the file is still available: on the
**Credentials** page, click the download icon at the right of your client's row.

:::caution The client secret is not a password to share
Google issues a `client_secret` for a Desktop client and requires it when
exchanging the authorization code, even though Apollia also uses PKCE. Google's
own documentation states that this value is not treated as confidential for
installed applications. It is still yours: keep the JSON file to yourself, and
do not commit it to a repository.
:::

## Step 5. Hand the client to Apollia

1. In Apollia, open **Settings**, then **OAuth integrations**.
2. On the **Google Workspace** card, click **Import JSON** and select the file
   you downloaded. The client ID and the client secret are read from it and
   written to `~/.apollia/oauth-clients.toml`, a file readable by your user only.
3. Click **Test configuration**. It should report the client present,
   well-formed, and Google's authorization server reachable.

If you would rather type the two values by hand, the fields on the same card
accept them directly. The client ID ends in `.apps.googleusercontent.com` and the
secret starts with `GOCSPX-`.

You are done in the console. To connect an account, go to **Connections**,
select **Google Workspace**, and follow
[Connect Google Workspace](/operator-help/integrations/connect-google-workspace).

## What Testing status costs you

Leaving the consent screen in **Testing** is free and immediate. It has two
limits, and the second one surprises people:

- **100 test users maximum.** Not a concern for personal use.
- **Refresh tokens expire after seven days.** Apollia will ask you to reconnect
  the account about once a week. Nothing is lost when it happens, you click
  through the consent page again.

Moving the consent screen to **Production** without verification removes the
seven-day expiry, at the cost of an "Google hasn't verified this app" warning
you have to expand and click through once per connection. To get rid of both,
submit the screen for verification: it is free for the scopes Apollia requests by
default, and takes several weeks.

The restricted scopes (`gmail.readonly`, `gmail.modify`, `gmail.compose`, and
full Drive access) are a separate matter. Those require a CASA Tier 2 security
assessment billed by a Google-approved third party, and Apollia does not request
them by default for that reason.

## Why Google asks for this and Microsoft does not

The two providers draw the line in different places.

Microsoft treats a desktop application as a **public client**: it holds no
secret, proves each request with PKCE, and its application identifier is a public
GUID that anyone can read out of the binary. Apollia can therefore register one
application and ship its identifier, which is why Microsoft 365 connects with
nothing to configure.

Google requires two things Apollia cannot satisfy on your behalf. Its consent
screen must pass verification before the application may serve accounts outside
its own project, and its Desktop client type requires a client secret at the
token endpoint, which no distributed binary can keep. A single shared Apollia
client would also put every user behind one quota and one consent screen.

So the ten minutes above buy you an identity you own, with your own quota, that
no one else's usage can throttle or get suspended.

## If it does not work

- **"Access blocked: Apollia has not completed the Google verification
  process"**: the address you are connecting is not in the test users list. Go
  back to step 3 and add it.
- **"Error 400: redirect_uri_mismatch"**: the client was created as a *Web
  application* rather than a **Desktop app**. Create a new client with the right
  type; you cannot change the type of an existing one.
- **Apollia refuses to connect and names a missing secret**: the client ID was
  entered by hand without its secret. Import the JSON file instead, or paste the
  secret into the second field.
- **An agent gets a permission error on one service only**: that service's API
  was never enabled in step 2. Enable it and retry, no reconnection needed.
- **You are asked to reconnect every week**: expected in Testing status, see
  above.
