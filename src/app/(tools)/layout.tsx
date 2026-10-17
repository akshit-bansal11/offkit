import { PlatformGate } from "@/components/site/PlatformGate";

export default function ToolsLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return <PlatformGate>{children}</PlatformGate>;
}
