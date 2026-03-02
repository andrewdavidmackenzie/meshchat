# HELP just in case you need it

## Location of Config files

The config file, which is in TOML format, (which, if things work well, you shouldn't need to edit) location
is platform-dependent and is thus:

- Windows: `C:\Users\{username}\AppData\Roaming\Mackenzie Serres\meshchat\config\config.toml`
- Linux: `~/.config/meshchat/config.toml`
- macOS: `$HOME/Library/Application Support/net.Mackenzie-Serres.meshchat/config.toml`

## Bluetooth issues on Linux

When starting work on a new Linux install, I had a number of issues that I had to overcome before meshchat could discover compatible radios. If you have some bluetooth devices working on your linux machine, you have probably overcome them already, but offering here just in case

- `bluetooth` user group doesn't exist.
    - FIND: trying to add yourself to the bluetooth user group will give an error:
        - `sudo usermod -a -G bluetooth $USER`
    - FIX:
        - `sudo groupadd bluetooth`

- Your user is not part of the `bluetooth` user group
    - FIND: list your user groups
        - `groups $USER`
        - If `bluetooth` is not one of those list then you need to fix it
    - FIX: 
        - `sudo usermod -a -G bluetooth $USER`
    - CHECK: List the groups your used is in again
        - `groups $USER`
        - `bluetooth` should now be one of the groups listed

- `org.bluez` Service is not registered with `DBus`
    - FIND: List the DBus registered services with:
        - `dbus-send --system --print-reply --dest=org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus.ListNames`
        - If `"org.bluez"` is not listed, then it is not registered with `DBus`
        - Check it is installed with `sudo apt install bluez`
    - FIX: Start the bluetooth service and check its status
        - `sudo systemctl start bluetooth`
        - `sudo systemctl status bluetooth`
    - CHECK: Rerun the check on list of services registered with `DBus`
        - `dbus-send --system --print-reply --dest=org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus.ListNames`
        - `org.bluez` should now be listed
