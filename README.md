# MeshChat
An Iced cross-platform GUI application to interact with Meshtastic LoRa radios:
- find Bluetooth Low Energy attached meshtastic devices
- connect to one
- use it to chat with others using the available channels

## Thinking
My current thinking is to keep it as simple to look at and use as possible, not being extremely geeky LoRa/Mesh
app and try to give users a simple chat experience, similar to ones they will be accustomed to with WhatsApp, Telegram
etc.

That will probably mean that for a while at least, they will need to use some other app to configure their radio
and join a mesh.

## Supported OS
In theory, using Iced, and Meshtastic rust crate and other dependencies that are all cross-platform, this app
should run on many OS, including macOS, Windows and Linux.

So far, I have used it successfully on 
* macOS (Tahoe)
* Linux (Pop OS!)

If you successfully run it on other OS or variants of the above, drop me a message in the repo's 
[discussions](https://github.com/andrewdavidmackenzie/meshchat/discussions) with some details and I will add to a 
list of known working OS.

## Supported Radios
In theory, it should work with all Meshtastic radios that are supported by the Meshtastic rust crate. 

So far, I only have a LillyGo T-ECHO.

Again, if you get it working successfully with other radios, drop me a message in 
[discussions](https://github.com/andrewdavidmackenzie/meshchat/discussions) and I will create some
list of known working radios.

## Running
```
cargo run
```
