"""Explicit repository relocation, reverse relocation, and safe Work-link removal.

Never deletes source/data, enables plugins, or calls shell IPC. Activation is a
separate, approved action. No overwrite/merge behavior is provided.
"""
import argparse
from pathlib import Path

ID = "io.github.jwkicklighter.snipboard"


def relocate(source, destination, link):
    source, destination, link = Path(source), Path(destination), Path(link)
    if source.is_symlink() or not (source / ".git").is_dir():
        raise ValueError("source must be the real repository directory")
    if destination.exists() or destination.is_symlink():
        raise ValueError("destination already exists; never overwrite")
    if link != source and (link.exists() or link.is_symlink()):
        raise ValueError("link destination already exists")
    destination.parent.mkdir(parents=True, exist_ok=True)
    # rename is atomic on this filesystem; deliberately no cross-filesystem copy.
    source.rename(destination)
    try:
        link.symlink_to(destination, target_is_directory=True)
    except OSError:
        destination.rename(source)
        raise


def unlink_work(link, target):
    link, target = Path(link), Path(target)
    if not link.is_symlink() or link.resolve() != target.resolve():
        raise ValueError("refusing to remove anything except the expected Work symlink")
    link.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("install", "uninstall", "unlink-work"))
    args = parser.parse_args()
    work = Path.home() / "Work/omarchy-snipboard"
    installed = Path.home() / ".config/omarchy/plugins" / ID
    if args.action == "install":
        relocate(work, installed, work)
    elif args.action == "unlink-work":
        unlink_work(work, installed)
    else:
        # Disable and rescan separately with approval before moving active code.
        if work.is_symlink():
            if work.resolve() != installed.resolve():
                raise ValueError("unexpected Work link")
            work.unlink()
            try:
                installed.rename(work)
            except OSError:
                work.symlink_to(installed, target_is_directory=True)
                raise
        elif not work.exists():
            installed.rename(work)
        else:
            raise ValueError("Work path is occupied; refusing overwrite")
    print("Done. Source and prototype data preserved; activation unchanged.")


if __name__ == "__main__":
    main()
