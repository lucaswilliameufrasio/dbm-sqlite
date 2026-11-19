#!/usr/bin/env python3
"""Exercise the Linux Flutter plugin's CMake contract without a GUI."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def configure(plugin: Path, directory: Path, architecture: str) -> subprocess.CompletedProcess[str]:
    source = directory / "source"
    source.mkdir()
    (source / "CMakeLists.txt").write_text(
        "cmake_minimum_required(VERSION 3.10)\n"
        "project(bundle_test LANGUAGES NONE)\n"
        f'add_subdirectory("{plugin.as_posix()}" plugin)\n'
        "list(LENGTH dbm_sqlite_bundled_libraries library_count)\n"
        "if(NOT library_count EQUAL 2)\n"
        '  message(FATAL_ERROR "expected exactly two bundled libraries")\n'
        "endif()\n"
        "foreach(library IN LISTS dbm_sqlite_bundled_libraries)\n"
        "if(NOT EXISTS \"${library}\")\n"
        '  message(FATAL_ERROR "bundled library is missing")\n'
        "endif()\n"
        'message(STATUS "dbm-bundle=${library}")\n'
        "endforeach()\n",
        encoding="utf-8",
    )
    return subprocess.run(
        [
            "cmake",
            "-S",
            str(source),
            "-B",
            str(directory / "build"),
            "-DCMAKE_SYSTEM_NAME=Linux",
            f"-DCMAKE_SYSTEM_PROCESSOR={architecture}",
        ],
        capture_output=True,
        text=True,
        check=False,
    )


def run_installed_bundle(root: Path, directory: Path) -> None:
    source = directory / "source"
    source.mkdir()
    plugin = root / "flutter" / "dbm_sqlite" / "linux"
    header = root / "flutter" / "dbm_sqlite" / "ios" / "Frameworks" / "DbmSqlite.xcframework" / "ios-arm64" / "DbmSqlite.framework" / "Headers"
    probe = root / "tools" / "linux-bundle-probe.c"
    (source / "main.c").write_text(
        "int run_bundle_probe(const char *path);\n"
        "int main(int argc, char **argv) {\n"
        "  if (argc != 2) { return 2; }\n"
        "  return run_bundle_probe(argv[1]);\n"
        "}\n",
        encoding="utf-8",
    )
    (source / "CMakeLists.txt").write_text(
        "cmake_minimum_required(VERSION 3.13)\n"
        "project(bundle_probe LANGUAGES C)\n"
        'set(CMAKE_INSTALL_RPATH "$ORIGIN/lib")\n'
        f'add_subdirectory("{plugin.as_posix()}" plugin)\n'
        f'add_library(probe_loader SHARED "{probe.as_posix()}")\n'
        f'target_include_directories(probe_loader PRIVATE "{header.as_posix()}")\n'
        'set_target_properties(probe_loader PROPERTIES INSTALL_RPATH "$ORIGIN")\n'
        "target_link_libraries(probe_loader PRIVATE ${CMAKE_DL_LIBS})\n"
        "add_executable(bundle_probe main.c)\n"
        "target_link_libraries(bundle_probe PRIVATE probe_loader)\n"
        "install(TARGETS bundle_probe RUNTIME DESTINATION .)\n"
        "install(TARGETS probe_loader LIBRARY DESTINATION lib)\n"
        "foreach(bundled_library ${dbm_sqlite_bundled_libraries})\n"
        '  install(FILES "${bundled_library}" DESTINATION lib)\n'
        "endforeach()\n",
        encoding="utf-8",
    )
    build = directory / "build"
    bundle = directory / "bundle"
    for command in (
        ["cmake", "-S", str(source), "-B", str(build)],
        ["cmake", "--build", str(build)],
        ["cmake", "--install", str(build), "--prefix", str(bundle)],
    ):
        subprocess.run(command, check=True, capture_output=True, text=True)
    environment = os.environ.copy()
    environment.pop("LD_LIBRARY_PATH", None)
    environment.pop("LD_PRELOAD", None)
    environment.pop("DBM_SQLITE_LIB", None)
    result = subprocess.run(
        [str(bundle / "bundle_probe"), str(directory / "test.sqlite")],
        cwd=directory,
        env=environment,
        check=True,
        capture_output=True,
        text=True,
    )
    print(result.stdout.strip())
    library = bundle / "lib" / "libdbm_sqlite_flutter.so"
    library.rename(library.with_suffix(".so.disabled"))
    missing = subprocess.run(
        [str(bundle / "bundle_probe"), str(directory / "missing.sqlite")],
        cwd=directory,
        env=environment,
        check=False,
        capture_output=True,
        text=True,
    )
    if missing.returncode == 0 or "loading bundled library failed" not in missing.stderr:
        raise SystemExit(f"missing bundled library was not rejected:\n{missing.stderr}")
    if (directory / "missing.sqlite").exists():
        raise SystemExit("missing bundled library unexpectedly created a database")
    print("bundled ABI: missing library rejected without creating a database")


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix="dbm-linux-bundle-") as scratch:
        directory = Path(scratch)
        supported = directory / "supported"
        unsupported = directory / "unsupported"
        missing = directory / "missing"
        supported.mkdir()
        unsupported.mkdir()
        missing.mkdir()
        plugin = root / "flutter" / "dbm_sqlite" / "linux"
        result = configure(plugin, supported, "x86_64")
        if result.returncode != 0 or "dbm-bundle=" not in result.stdout:
            raise SystemExit(f"x86_64 CMake configuration failed:\n{result.stdout}\n{result.stderr}")
        result = configure(plugin, unsupported, "aarch64")
        output = result.stdout + result.stderr
        if result.returncode == 0 or "Linux x86_64 library only" not in output:
            raise SystemExit(f"unsupported architecture was not rejected:\n{output}")
        missing_plugin = missing / "plugin"
        missing_plugin.mkdir()
        shutil.copyfile(plugin / "CMakeLists.txt", missing_plugin / "CMakeLists.txt")
        result = configure(missing_plugin, missing, "x86_64")
        output = result.stdout + result.stderr
        if result.returncode == 0 or "prebuilt library is missing" not in output:
            raise SystemExit(f"missing prebuilt was not rejected at configuration:\n{output}")
        missing_cipher = directory / "missing-cipher"
        missing_cipher.mkdir()
        cipher_plugin = missing_cipher / "plugin"
        cipher_plugin.mkdir()
        shutil.copyfile(plugin / "CMakeLists.txt", cipher_plugin / "CMakeLists.txt")
        cipher_lib = cipher_plugin / "lib" / "x86_64"
        cipher_lib.mkdir(parents=True)
        shutil.copyfile(
            plugin / "lib" / "x86_64" / "libdbm_sqlite_flutter.so",
            cipher_lib / "libdbm_sqlite_flutter.so",
        )
        result = configure(cipher_plugin, missing_cipher, "x86_64")
        output = result.stdout + result.stderr
        if result.returncode == 0 or "SQLCipher library is missing" not in output:
            raise SystemExit(f"missing SQLCipher prebuilt was not rejected:\n{output}")
        installed = directory / "installed"
        installed.mkdir()
        run_installed_bundle(root, installed)
    print("Linux Flutter bundle: x86_64 configured; unsupported architecture and missing libraries rejected")


if __name__ == "__main__":
    main()
