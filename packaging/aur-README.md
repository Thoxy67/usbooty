# usbooty-git

Arch User Repository package for [usbooty](https://git.thoxy.xyz/thoxy/usbooty),
a Rufus-like tool that creates bootable USB drives from ISO images.

This is the **git** package: it builds the latest commit from upstream on
every invocation.

## Install

```sh
git clone https://git.thoxy.xyz/AUR/usbooty-git.git
cd usbooty-git
makepkg -fsi
```

## Optional runtime dependencies

`dosfstools`, `ntfs-3g`, `exfatprogs`, and `e2fsprogs` provide the respective
filesystem formatters. `wimlib` provides `wimlib-imagex` for splitting large
Windows `install.wim` files. `ventoy` enables the Ventoy method. Install
whichever you need.

## More information

* Source repository: <https://git.thoxy.xyz/thoxy/usbooty>
* Full documentation: see the [`docs/`](https://git.thoxy.xyz/thoxy/usbooty/src/branch/main/docs)
  directory in the source repo for architecture, write methods,
  Windows-specific behaviour, and troubleshooting.

## License

GPL-3.0-or-later. See the project repository.
