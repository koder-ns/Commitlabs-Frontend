import ProtectedRouteLayout from '@/components/auth/ProtectedRouteLayout'

export default function CommitmentsLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return <ProtectedRouteLayout>{children}</ProtectedRouteLayout>
}
