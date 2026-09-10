# Included after every project() call of whisper.cpp's CMake tree on Windows,
# through CMAKE_PROJECT_INCLUDE (set by host_runner_cmake_env in
# host_desktop_defaults.sh).
#
# ggml's Vulkan backend builds its shader generator as an ExternalProject,
# which nests a second CMake tree under
# ggml/src/ggml-vulkan/vulkan-shaders-gen-prefix/src/vulkan-shaders-gen-build,
# and that tree's compiler check writes an object file under
# CMakeFiles/CMakeScratch/TryCompile-<id>/CMakeFiles/<id>.dir/. From cargo's
# own output directory that path is 265 characters, past the 260 cl.exe still
# enforces whatever the machine's long-path setting, so the check failed with
# C1083 and no Vulkan runner could be built. Measured on 2026-09-10 on
# Windows 11 with long paths enabled, cmake 4.4, Visual Studio 2022 tools.
#
# EP_BASE is an inherited directory property ExternalProject reads when a call
# names no PREFIX, so setting it at the project root moves that tree to
# <build>/ep/Build/vulkan-shaders-gen and takes 41 characters off every path
# in it. Nothing else in the build changes.
set_property(DIRECTORY PROPERTY EP_BASE "${CMAKE_BINARY_DIR}/ep")
