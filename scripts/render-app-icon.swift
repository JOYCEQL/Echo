import AppKit

// Ask macOS to apply its application-icon treatment instead of sending the
// unmasked source image to NSApplication's runtime Dock icon override.
guard CommandLine.arguments.count == 3 else {
    fatalError("Usage: render-app-icon.swift bundle.app output.png")
}
let image = NSWorkspace.shared.icon(forFile: CommandLine.arguments[1])
image.size = NSSize(width: 512, height: 512)
guard let data = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: data),
      let png = bitmap.representation(using: .png, properties: [:]) else {
    fatalError("macOS could not render the application icon")
}
try png.write(to: URL(fileURLWithPath: CommandLine.arguments[2]))
