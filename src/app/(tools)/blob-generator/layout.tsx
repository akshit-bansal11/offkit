import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("blob-generator");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
