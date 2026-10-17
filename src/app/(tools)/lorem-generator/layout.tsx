import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("lorem-generator");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
