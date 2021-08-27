# Copyright 1999-2020 Gentoo Authors
# Distributed under the terms of the GNU General Public License v2

EAPI=7
inherit cargo

DESCRIPTION="My dupefinder!"
SRC_URI="https://github.com/installgentoo/dupefinder_advanced/archive/${PV}.tar.gz"

LICENSE="MIT"
RESTRICT="mirror"
SLOT="0"
KEYWORDS="~amd64 ~x86"
IUSE=""

RDEPEND="virtual/rust
		app-misc/blockhash
		media-libs/glfw"
DEPEND="${RDEPEND}"

src_unpack() {
	cargo_src_unpack
}

src_compile() {
	export CARGO_HOME="$(pwd)"
	export CARGO_TARGET_DIR="$(pwd)"
	cargo build --release
	cargo install
}

src_install() {
	insinto /usr/bin/
	insopts -m0755
	doins release/dedup_adv
}
