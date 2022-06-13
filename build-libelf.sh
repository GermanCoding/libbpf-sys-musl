#!/bin/bash
# Safety check: Are we in the directory we assume we are?
if [ ! -e ./build-libelf.sh ]; then
   echo "Error: build-libelf working directory not set to script directory. Aborting"
   exit 1
fi

# Does libs directory exist and is not empty?
if [ -d libs ]; then
   if [ -e libs/libelf.a ]; then
      echo "Info: libelf binary seems to already exist, skipping build. Remove libs directory to trigger a rebuild."
      exit 0
   fi
fi

if [ "$1" == "" ] || [ $# -gt 1 ]; then
   echo "Usage: ./build-libelf.sh <target>"
   echo "Example target: x86_64-linux-musl (note that this script is only intended for musl targets)"
   exit 1
fi
target=$1
cross_toolchain="${target}-cross"
echo "Setting target to $target"

if [ "${CC}" == "" ] || [ $# -gt 1 ]; then
   echo "Error: You must set a compiler via enviroment variable CC"
   echo "Example: CC=musl-gcc ./build-libelf.sh x86_64-linux-musl"
   exit 1
fi

# Ensure static option is set
CC="${CC} -static"
echo "Setting compiler: $CC"

required_commands=( "wget" "tar" "ln" "git" "aclocal" "autoconf" "autoheader" "automake" "make" "libtoolize" "autoreconf" )

for cmd in "${required_commands[@]}"
do
   if ! command -v $cmd &> /dev/null
   then
      echo "Error: Can't build libelf: This script requires $cmd"
      exit 1
   fi
done

# Clean everything
rm -rf ./libs
rm -rf ./include
rm -rf ./build-tmp
mkdir ./libs
mkdir ./include
mkdir ./build-tmp

# No command starting at this point may fail
set -e
base_dir=$PWD

# We need to setup linux headers that may not be provided by the given musl toolchain
# The builds from https://musl.cc/ should include everything we need, and more.
# Note that these builds tend to target a recent kernel. If you have different needs you might need to set a different toolchain here.
wget https://musl.cc/"${cross_toolchain}.tgz" -O ./build-tmp/toolchain.tgz
tar -xf ./build-tmp/toolchain.tgz -C ./build-tmp/
include_dir="${PWD}/build-tmp/${cross_toolchain}/${target}/include"
cp -ar "${include_dir}/asm" ./include/asm
cp -ar "${include_dir}/asm-generic" ./include/asm-generic
cp -ar "${include_dir}/linux" ./include/linux

cd ./build-tmp

# Dependency: argp_standalone
# libelf depends on argp (normally provided by glibc), but musl doesn't have that. So we require a standalone dependency for this.
git clone --depth 1 https://github.com/ericonr/argp-standalone.git
cd ./argp-standalone
# Automake flow
aclocal
autoconf
autoheader
automake --add-missing
CC=$CC ./configure
make
cp -a ./libargp.a "${base_dir}/libs/"
cp -a ./argp.h "${base_dir}/include/"
cd ..

# Dependency: libz
# Libz is required by both libelf and libbpf(-sys)
git clone --depth 1 https://github.com/madler/zlib.git
cd ./zlib
CC=$CC ./configure --static
make
cp -a ./libz.a "${base_dir}/libs/"
cp -a ./zlib.h "${base_dir}/include/"
cp -a ./zconf.h "${base_dir}/include/"
cd ..

# Dependency: FTS
# libelf depends on FTS (normally provided by glibc), but musl doesn't provide it. So we require a standalone dependency for this.
git clone --depth 1 https://github.com/void-linux/musl-fts.git
cd ./musl-fts
./bootstrap.sh
CC=$CC ./configure
make
cp -a ./.libs/libfts.a "${base_dir}/libs/"
cp -a ./fts.h "${base_dir}/include/"
cd ..

# Dependency: obstack
# libelf depends on obstack (normally provided by glibc), but musl doesn't provide it. So we require a standalone dependency for this.
git clone --depth 1 https://github.com/void-linux/musl-obstack.git
cd ./musl-obstack
./bootstrap.sh
CC=$CC ./configure
make
cp -a ./.libs/libobstack.a "${base_dir}/libs/"
cp -a ./obstack.h "${base_dir}/include/"
cd ..

# Finally, build elfutils (libelf)
git clone --depth 1 git://sourceware.org/git/elfutils.git
cd elfutils
autoreconf -i -f
CC=$CC LDFLAGS="-L${base_dir}/libs" CFLAGS="-I${base_dir}/include" ./configure --enable-maintainer-mode --disable-libdebuginfod --disable-debuginfod
cd libelf
make libelf.a
cd ..
cp -a ./libelf/libelf.a "${base_dir}/libs/"
cp -a ./libelf/libelf.h "${base_dir}/include/"
cp -a ./libelf/gelf.h "${base_dir}/include/"
cd ..

cd ..
rm -rf ./build-tmp
