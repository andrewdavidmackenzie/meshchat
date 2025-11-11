# MeshChat

*** WARNING: Alpha quality, and no automated tests yet! ***

(but since it doesn't try to change configuration of attached radio, it's kind of low risk)

An Iced cross-platform GUI application to interact with Meshtastic LoRa radios:

- find Bluetooth Low Energy attached meshtastic devices
- connect to one
- use it to chat with others using the available channels or direct messages to Nodes
- it saves the last device connected to (and channel if applicable) and on re-start it will try to
  automatically reconnect to that and continue chatting

## Thinking

My current thinking is to keep it as simple to look at and use as possible.

- avoid the app being an extremely geeky LoRa/Mesh app.
- try to give users a simple chat experience, similar to ones they will be accustomed to with WhatsApp, Telegram,
  etc.
- you will need to use some other app to configure their radio and join a mesh

## Supported OS

In theory, using Iced and Meshtastic rust crate and other dependencies that are all cross-platform, this app
should run on many Operating Systems, including macOS, Windows and Linux.

So far, I have used it successfully on

* macOS (Tahoe)
* Linux (Pop OS!)
    * Known [bug](https://github.com/andrewdavidmackenzie/meshchat/issues/16) in BLE device discovery - it detects all
      BlueTooth devices, not just Meshtastic radios

If you successfully run it on other OS or variants of the above, drop me a message in the repo's
[discussions](https://github.com/andrewdavidmackenzie/meshchat/discussions) with some details, and I will add to a list
of known working OS.

## Supported Radios

In theory, it should work with all Meshtastic radios that are supported by the Meshtastic rust crate.

So far, I have tested with a LillyGo T-ECHO and a T-Deck Pro

Again, if you get it working successfully with other radios, drop me a message in
[discussions](https://github.com/andrewdavidmackenzie/meshchat/discussions) and I will create some list of known working
radios.

## Installing

`cargo install meshchat` will get you the binary installed and in your path thanks to cargo, if you have a working
rust toolchain installed.

Later I may work on pre-build binaries attached to GitHub releases, or `cargo binstall`support to make it even easier.

### Binary Size

After a bit of optimizing I did, the binary size should be around 5.6M

## Running

If you clone the repo, you can run it directly with:

`cargo run --release`

## Licensing

These are the top-level dependencies of meshchat and their licenses:

* meshtastic - [GPL-3.0 License](https://github.com/meshtastic/rust/blob/main/LICENSE)
* iced - [MIT License](https://github.com/iced-rs/iced/blob/master/LICENSE)
* iced_futures - Inherits from iced, so MIT
* directories - Apache Version 2, or MIT license at your option

So, due to the meshtastic license, this project is also licensed under the [GPL-3.0 License](LICENSE)