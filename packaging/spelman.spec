Name:           spelman
Version:        0.1.0
Release:        1%{?dist}
Summary:        A terminal music player written in Rust
License:        MIT
URL:            https://github.com/petterssonjonas/Spelman
Source0:        %{url}/archive/refs/tags/v%{version}.tar.gz#/Spelman-%{version}.tar.gz

BuildRequires:  cargo >= 1.82
BuildRequires:  rust >= 1.82
BuildRequires:  alsa-lib-devel
BuildRequires:  gcc

%description
Spelman is a terminal music player written in Rust. It supports MP3, FLAC,
OGG, Opus, WAV, and AAC playback with a TUI interface, vim-style keybindings,
and low-latency audio via a lock-free ring buffer.

%prep
%autosetup -n Spelman-%{version}

%build
cargo build --release

%install
install -Dm755 target/release/spelman %{buildroot}%{_bindir}/spelman
install -Dm644 packaging/com.github.spelman.desktop %{buildroot}%{_datadir}/applications/com.github.spelman.desktop

%files
%license LICENSE
%doc README.md
%{_bindir}/spelman
%{_datadir}/applications/com.github.spelman.desktop

%changelog
* Sun Mar 16 2026 Jonas Pettersson <noreply@github.com> - 0.1.0-1
- Initial package — Phase 1: single-file playback with TUI
