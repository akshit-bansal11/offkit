import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("base64");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
