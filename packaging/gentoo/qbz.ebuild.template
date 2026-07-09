# Copyright 2024-2026 Gentoo Authors
# Distributed under the terms of the MIT License

EAPI=8

inherit xdg

DESCRIPTION="Native hi-fi Qobuz desktop player for Linux"
HOMEPAGE="https://qbz.lol https://github.com/vicrodh/qbz"

MY_PV="${PV}"
SRC_URI="https://github.com/vicrodh/qbz/archive/refs/tags/v${MY_PV}.tar.gz -> ${P}.tar.gz"

LICENSE="MIT"
SLOT="0"
KEYWORDS="~amd64 ~arm64"
IUSE="pipewire pulseaudio"
RESTRICT="mirror network-sandbox"

# v2.0+ (Slint/winit binary): no webkit/gtk/appindicator/node.
RDEPEND="
	media-libs/alsa-lib
	media-libs/fontconfig
	media-libs/freetype
	media-libs/libpng
	app-arch/bzip2
	dev-libs/expat
	sys-libs/zlib
	x11-libs/libxkbcommon
	dev-libs/wayland
	media-libs/libglvnd
	sys-apps/dbus
	pipewire? (
		media-video/pipewire[pipewire-alsa]
	)
	pulseaudio? (
		media-libs/libpulse
	)
"

BDEPEND="
	dev-lang/rust
	llvm-core/clang
	dev-lang/nasm
	virtual/pkgconfig
"

PDEPEND="
	media-sound/alsa-utils
"

S="${WORKDIR}/qbz-${MY_PV}"

src_compile() {
	# Symbols stripped by the workspace [profile.release]. The qbz_ui rustc
	# peaks ~30 GB RAM — single job keeps the footprint at its measured MIN.
	CARGO_BUILD_JOBS=1 cargo build --release \
		--manifest-path crates/Cargo.toml -p qbz || die "cargo build failed"
}

src_install() {
	dobin "crates/target/release/qbz"

	insinto /usr/share/applications
	newins packaging/linux/qbz.desktop qbz.desktop

	local size
	for size in 32 48 64 128 256; do
		local src_icon="packaging/icons/${size}x${size}.png"
		if [[ -f "${src_icon}" ]]; then
			insinto "/usr/share/icons/hicolor/${size}x${size}/apps"
			newins "${src_icon}" qbz.png
		fi
	done
}

pkg_postinst() {
	xdg_icon_cache_update
	xdg_desktop_database_update

	elog "For bit-perfect audio playback, ensure your user is in the 'audio' group:"
	elog "  usermod -aG audio \${USER}"
	elog ""
	elog "Optional: install media-sound/alsa-utils for better ALSA device detection."
}

pkg_postrm() {
	xdg_icon_cache_update
	xdg_desktop_database_update
}
