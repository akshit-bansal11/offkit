import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("video-editor");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
