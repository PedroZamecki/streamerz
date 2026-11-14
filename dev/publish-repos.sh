#!/bin/sh
# At a release: signs what dev/packages.sh made and lays out the GitHub Pages site the packages update
# from, and makes the .flatpak (which adds that site as its remote, so Flatpak updates it):
#   target/site/streamerz.asc          the public key (packaging/streamerz.asc)
#   target/site/apt/                   a flat APT repository (InRelease signed)
#   target/site/rpm/                   a DNF/zypper repository (repomd.xml signed)
#   target/site/arch/                  a pacman repository (packages and database signed)
#   target/site/flatpak/repo/          the Flatpak repository (commits and summary signed)
#   target/site/streamerz.flatpakrepo, streamerz.flatpakref, index.html
# The signing key is in gpg's keyring (CI imports the PACKAGES_GPG_KEY secret). Only the newest
# release is kept: updaters only need it. Nothing in the packages changes; signatures sit beside them.
# Usage: dev/publish-repos.sh <version> [folder with the release files]   (needs flatpak, gpg,
# apt-ftparchive, createrepo_c, and repo-add or docker)
set -eu
ver=${1:?version}
clean=${ver#v}
files=${2:-dist}
id=io.github.pedrozamecki.streamerz
url=https://pedrozamecki.github.io/streamerz
site=target/site
key=packaging/streamerz.asc
fpr=$(gpg --with-colons --import-options show-only --import "$key" | awk -F: '/^fpr/ { print $10; exit }')
sign="gpg --batch --yes --local-user $fpr"

rm -rf "$site" target/flatpak
mkdir -p "$site/apt" "$site/rpm" "$site/arch" "$site/flatpak" target/flatpak
cp "$key" "$site/streamerz.asc"

# APT: a flat repository (Suites: ./), as packaging/streamerz.sources names it.
cp "$files/Streamerz-linux-x86_64.deb" "$site/apt/streamerz_${clean}_amd64.deb"
(
    cd "$site/apt"
    apt-ftparchive packages . > Packages
    gzip -9kn Packages
    apt-ftparchive -o APT::FTPArchive::Release::Origin=Streamerz -o APT::FTPArchive::Release::Label=Streamerz \
        release . > Release
    $sign --clearsign --output InRelease Release
    $sign --armor --detach-sign --output Release.gpg Release
)

# DNF and zypper: the metadata is signed, and it holds every package's checksum.
cp "$files/Streamerz-linux-x86_64.rpm" "$site/rpm/streamerz-$clean.x86_64.rpm"
createrepo_c --quiet "$site/rpm"
$sign --armor --detach-sign --output "$site/rpm/repodata/repomd.xml.asc" "$site/rpm/repodata/repomd.xml"

# pacman: the package and the database signed (pacman's default SigLevel wants both checked).
pkg="streamerz-$clean-1-x86_64.pkg.tar.zst"
cp "$files/Streamerz-linux-x86_64.pkg.tar.zst" "$site/arch/$pkg"
$sign --detach-sign --output "$site/arch/$pkg.sig" "$site/arch/$pkg"
if command -v repo-add > /dev/null; then
    (cd "$site/arch" && repo-add --quiet streamerz.db.tar.gz "$pkg")
else
    docker run --rm -v "$PWD/$site/arch:/repo" -w /repo archlinux:latest repo-add --quiet streamerz.db.tar.gz "$pkg"
fi
# GitHub Pages serves no links: the names pacman asks for are copies.
for f in db files; do
    rm -f "$site/arch/streamerz.$f"
    cp "$site/arch/streamerz.$f.tar.gz" "$site/arch/streamerz.$f"
done
$sign --detach-sign --output "$site/arch/streamerz.db.sig" "$site/arch/streamerz.db"
cp "$site/arch/streamerz.db.sig" "$site/arch/streamerz.db.tar.gz.sig"

# Flatpak: sign the commit and the summary, then the bundle, which adds this remote when installed.
tar -C target/flatpak -xf "$files/streamerz-flatpak-repo.tar"
repo=target/flatpak/repo
flatpak build-sign --gpg-sign="$fpr" "$repo" "$id" stable
flatpak build-update-repo --gpg-sign="$fpr" --title=Streamerz --default-branch=stable --prune "$repo"
gpg --export "$fpr" > target/flatpak/key.gpg
flatpak build-bundle --repo-url="$url/flatpak/repo/" \
    --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    --gpg-keys=target/flatpak/key.gpg "$repo" "$files/Streamerz-linux-x86_64.flatpak" "$id" stable
cp -a "$repo" "$site/flatpak/repo"
gpgkey=$(base64 -w0 target/flatpak/key.gpg)
cat > "$site/streamerz.flatpakrepo" << EOF
[Flatpak Repo]
Title=Streamerz
Url=$url/flatpak/repo/
Homepage=https://github.com/PedroZamecki/streamerz
Icon=$url/streamerz.svg
GPGKey=$gpgkey
EOF
cat > "$site/streamerz.flatpakref" << EOF
[Flatpak Ref]
Title=Streamerz
Name=$id
Branch=stable
Url=$url/flatpak/repo/
SuggestRemoteName=streamerz
Homepage=https://github.com/PedroZamecki/streamerz
Icon=$url/streamerz.svg
RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
IsRuntime=false
GPGKey=$gpgkey
EOF
cp assets/brand/streamerz.svg "$site/streamerz.svg"
sed -e "s/@VERSION@/$clean/g" -e "s/@FINGERPRINT@/$fpr/g" packaging/site.html > "$site/index.html"
# The repository tar was only for this: it isn't a release file.
rm "$files/streamerz-flatpak-repo.tar"
du -sh "$site"
