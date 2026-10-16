import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("video-converter");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
