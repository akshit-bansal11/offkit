import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("image-cropper");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
