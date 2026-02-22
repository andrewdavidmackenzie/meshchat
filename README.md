# ![MeshChat Logo](assets/images/small_icon.png) MeshChat

[![codecov](https://codecov.io/gh/andrewdavidmackenzie/meshchat/branch/main/graph/badge.svg?token=VYddKXC8mh)](https://codecov.io/gh/andrewdavidmackenzie/meshchat)

Meshchat is a cross-platform GUI application to interact with Meshtastic and MeshCore LoRa radios:

- discover Bluetooth Low Energy attached compatible radios
- connect to one
- use it to chat with others using preset Channels or direct messages to Nodes
- it saves the last device connected to (and Channel/Node if chosen), and on re-start it will try to
  automatically reconnect to that device and channel/node and continue chatting

## Installers

Installers for macOS, Linux and Windows are available
at [Latest Release on GitHub](https://github.com/andrewdavidmackenzie/meshchat/releases/latest)

## Screenshots

On the left, the Device view, once you have connected to a BLE Radio, shows configured channels, a list of nodes
found and nodes you have marked as a favorite.

On the right, the Channel view, Once you have clicked on a channel or a node, shows you the ongoing chat messages with
it,
with your messages on the right and others on the left.

<!--suppress ALL -->
<table cellspacing="0" cellpadding="0" border="0">
  <tr>
    <td valign="top">
      <img alt="Device View" src="./assets/images/Device_View.png" width="400" align="top" />
    </td>
    <td valign="top">
      <img alt="Channel View" src="./assets/images/Channel_View.png" width="400" align="top" />
    </td>
  </tr>
</table>

## The Thinking

My thinking was to keep the app as simple to look at and use as possible.

- avoid the app being an extremely geeky LoRa/Mesh app.
- try to give users a simple chat experience, similar to ones they will be accustomed to with WhatsApp, Telegram,
  etc.
- users will need to use another app to configure their radio

## Newer Features

Here are some of the features I have added in recent releases:

### 0.3.6 Release ([Release Notes](https://github.com/andrewdavidmackenzie/meshchat/releases/tag/0.3.6-rc1))

The main addition in this release is initial support for MeshCore radios, and basic chatting with Channels or Nodes
("Contacts" in MeshCore).

### 0.3.0 Release ([Release Notes](https://github.com/andrewdavidmackenzie/meshchat/releases/tag/0.3.0-rc4))

* Added windows-11-arm to matrix for build and release
* Added ubuntu-24.04-arm to build and release
* Added two required packages as dependencies to .deb packages, which should cause them to be installed when needed
* Important bug fixes related to Bluetooth discovery, and "undiscovery" of radios
* Fix bugs in discovery code that caused crashes on Linux.
* Reworked styles to follow the selected Dark or Light Theme (via System).
* Many smaller UI changes and bug fixes you can find details of in the release notes
* Large increase in test coverage

### 0.2.1 Release ([Release Notes](https://github.com/andrewdavidmackenzie/meshchat/releases/tag/0.2.1))

Adds a Windows MSI installer for Windows x86.

### 0.2.0 Release ([Release Notes](https://github.com/andrewdavidmackenzie/meshchat/releases/tag/0.2.0))

This release includes:

- Discover nearby MeshTastic compatible radios via Bluetooth and list them in the Device View
- Connect to a MeshTastic radio, then view a list of Channels and Nodes it knows about
- Save the last device connected to (and channel if applicable), and on re-start automatically reconnect to it and
  open the channel
- Filter the list of Channels and Nodes by name
- Start a chat with a Channel or a Node, viewing messages received and send new messages
  (Text, Text Reply, Position, Alert, NodeInfo)
- Acknowledgement indicator on a message to show it was received by the other side
- Unread message count indicator on Channels, Nodes and Device overall
- macOS and Linux application bundles are included in the GH Release Artifacts
- Ability to Reply to a message, show replies quoting the original message
- Ability to Forward a message to another Channel or Node
- Ability to Copy a message to the clipboard to be pasted elsewhere
- Ability to React to a message with an emoji
- Ability to start a DM with a Node from its name in a message
- Empty views for Device List and Channel/Node View when there is nothing to see, with some instructions
- Ability to Send your radio's current position
- Ability to Send your node's info
- Show the battery level of the connected radio in the header
- Ability to alias a BlueTooth Device with a more friendly or memorable name of your choosing
- Ability to alias a Node with a more friendly or memorable name of your choosing
- Ability to favourite Nodes and show the list of Favourite nodes at the top of the Device View
- Button on each node in the Device View to allow you to see its position (on Google Maps)

## Discussions [link](https://github.com/andrewdavidmackenzie/meshchat/discussions)

Raise questions or discuss ideas or issues in discussions.

I am interested in hearing users' thoughts
about [storing chat history locally](https://github.com/andrewdavidmackenzie/meshchat/discussions/145)

## Supported OS

Meshchat should run on macOS, Windows and Linux. So far, it has been confirmed to run correctly on:

* macOS (Tahoe, Apple Silicon / arm64)
* Linux (x86 and arm64)
    * Pi500 running PiOS (there are still problems with Pi4/400 I am working on)
    * There is a known [bug](https://github.com/andrewdavidmackenzie/meshchat/issues/16) in BLE device discovery—it
      detects all BlueTooth devices, not just compatible radios
* Windows 11 (both x86 and arm64)

If you successfully run it on other OS or variants of the above, drop me a message in the repo's
[discussions](https://github.com/andrewdavidmackenzie/meshchat/discussions) with some details, and I will add to a list.

## Supported Radios

It should work with all Meshtastic radios that are supported by the Meshtastic rust crate.

- So far, I have tested with a LillyGo T-ECHO, a T-Deck Pro and Heltec V3 running Meshtastic

It should work with all MeshCore radios

- but so far I have only worked with a Heltec V3 running MeshCore (Ripple).
- I created the [meshcore-rs](https://github.com/andrewdavidmackenzie/meshcore-rs) crate to discover,
  connect and communicate with MeshCore radios.

If you use it successfully with other radios, drop me a message in
[discussions](https://github.com/andrewdavidmackenzie/meshchat/discussions) and I will create some list of known working
radios.

## Installing

After a bit of optimizing I did, the release binary size is around 5.6 MBytes

### Download an installer

Each release contains installers for macOS, Linux and Windows that you can download and run from
[Latest Release on GitHub](https://github.com/andrewdavidmackenzie/meshchat/releases/latest)

### Cargo install

If you have a working rust toolchain installed, you can use:

`cargo install meshchat`

to get the binary built and installed and in your `$PATH`.

### Build and run from source

Clone the repo, then run meshchat directly with:

`cargo run --release`

## Contributing

If you want to help out, submit a well-written [issue](https://github.com/andrewdavidmackenzie/meshchat/issues), start
or participate in
a [discussion](https://github.com/andrewdavidmackenzie/meshchat/discussions) or fork the repo and submit a PR.

### Known Bugs

Here is the list
of [Known Bugs](https://github.com/andrewdavidmackenzie/meshchat/issues?q=is%3Aissue%20state%3Aopen%20label%3Abug)

## CI Testing

Tests are run in GitHub actions on macos-15 (arm64), ubuntu-latest (x86), ubuntu-arm, window-latest (x86)
and windows-11-arm (arm64).

## Licensing

This project is also licensed under the [GPL-3.0 License](LICENSE)
