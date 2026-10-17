import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("audio-editor");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
