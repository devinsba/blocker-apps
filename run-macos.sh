#!/bin/bash

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DESKTOP_APP_DIR="$SCRIPT_DIR/desktop/flutter_app"
SHARED_MODULE_DIR="$SCRIPT_DIR/shared/blocker_shared"

echo -e "${BLUE}=====================================${NC}"
echo -e "${BLUE}  Blocker macOS App Runner${NC}"
echo -e "${BLUE}=====================================${NC}\n"

# Function to print status messages
print_status() {
    echo -e "${BLUE}[*]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[✓]${NC} $1"
}

print_error() {
    echo -e "${RED}[✗]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[!]${NC} $1"
}

# Check if Flutter is installed
print_status "Checking for Flutter..."
if ! command -v flutter &> /dev/null; then
    print_error "Flutter is not installed or not in PATH"
    echo -e "\nPlease install Flutter from: https://flutter.dev/docs/get-started/install"
    exit 1
fi
print_success "Flutter found: $(flutter --version | head -n 1)"

# Check Flutter doctor
print_status "Running Flutter doctor..."
if ! flutter doctor | grep -q "macOS toolchain"; then
    print_warning "macOS toolchain may not be properly configured"
    flutter doctor
    echo ""
    read -p "Continue anyway? (y/n) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
else
    print_success "macOS toolchain is configured"
fi

# Check if macOS project exists
print_status "Checking if macOS project is initialized..."
if [ ! -d "$DESKTOP_APP_DIR/macos" ]; then
    print_warning "macOS project not found. Initializing..."
    cd "$DESKTOP_APP_DIR" || exit 1

    if flutter create --platforms=macos --overwrite .; then
        print_success "macOS project initialized"
    else
        print_error "Failed to initialize macOS project"
        exit 1
    fi
else
    print_success "macOS project exists"
fi

# Get dependencies for shared module
print_status "Getting dependencies for shared module..."
cd "$SHARED_MODULE_DIR" || exit 1
if flutter pub get; then
    print_success "Shared module dependencies installed"
else
    print_error "Failed to get shared module dependencies"
    exit 1
fi

# Get dependencies for desktop app
print_status "Getting dependencies for desktop app..."
cd "$DESKTOP_APP_DIR" || exit 1
if flutter pub get; then
    print_success "Desktop app dependencies installed"
else
    print_error "Failed to get desktop app dependencies"
    exit 1
fi

# Check if pubspec.yaml has path_provider dependency
print_status "Checking desktop app dependencies..."
if ! grep -q "path_provider:" "$DESKTOP_APP_DIR/pubspec.yaml"; then
    print_warning "Adding path_provider dependency..."
    # Add path_provider if not present
    if ! grep -q "dependencies:" "$DESKTOP_APP_DIR/pubspec.yaml"; then
        echo -e "\ndependencies:\n  path_provider: ^2.1.1" >> "$DESKTOP_APP_DIR/pubspec.yaml"
    fi
    flutter pub add path_provider
    print_success "path_provider added"
fi

# Validate that shared module is properly linked
print_status "Validating shared module dependency..."
if ! grep -q "blocker_shared:" "$DESKTOP_APP_DIR/pubspec.yaml"; then
    print_warning "Shared module not linked in desktop app pubspec.yaml"
    print_status "Adding shared module dependency..."

    # Add dependency to pubspec.yaml
    cat >> "$DESKTOP_APP_DIR/pubspec.yaml" << EOF

  blocker_shared:
    path: ../../shared/blocker_shared
EOF

    flutter pub get
    print_success "Shared module linked"
else
    print_success "Shared module is properly linked"
fi

# Check for macOS devices
print_status "Checking for available macOS devices..."
if ! flutter devices | grep -q "macos"; then
    print_error "No macOS device available"
    exit 1
fi
print_success "macOS device found"

# Clean build (optional)
if [ "$1" == "--clean" ]; then
    print_status "Cleaning build artifacts..."
    flutter clean
    flutter pub get
    print_success "Build cleaned"
fi

# Build or run
echo ""
if [ "$1" == "--build" ]; then
    print_status "Building macOS app..."
    echo -e "${YELLOW}This may take a few minutes...${NC}\n"

    if flutter build macos; then
        print_success "Build completed successfully!"
        echo -e "\n${GREEN}App location:${NC} $DESKTOP_APP_DIR/build/macos/Build/Products/Release/flutter_app.app"
    else
        print_error "Build failed"
        exit 1
    fi
else
    print_status "Running macOS app..."
    echo -e "${YELLOW}Starting Flutter in debug mode...${NC}\n"
    echo -e "${BLUE}=====================================${NC}\n"

    flutter run -d macos
fi
