import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("pdf-compressor");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
