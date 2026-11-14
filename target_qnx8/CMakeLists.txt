cmake_minimum_required(VERSION 3.20)
project(srr_resmgr_demo LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# BlackBerry QNX Neutrino 8.0 SDP Toolchain Configuration
# Usage:
#   source ~/qnx800/qnxsdp-env.sh
#   cmake -B build -DCMAKE_TOOLCHAIN_FILE=${QNX_TARGET}/usr/share/cmake/qnx.nto.toolchain.cmake -DARCH=aarch64
#   cmake --build build

if(CMAKE_SYSTEM_NAME STREQUAL "QNX")
    message(STATUS "[QNX 8.0] Configuring native build for Neutrino OS")
    add_compile_options(-Wall -Wextra -Wpedantic -O3)
    # Link against QNX microkernel socket and trace libraries
    link_libraries(socket)
else()
    message(STATUS "[HOST EMULATION] Configuring for host stub / smoke testing")
    add_compile_options(-Wall -Wextra -O3)
endif()

add_executable(srr_resmgr_demo srr_resmgr_demo.cpp)
