import { Alert, Anchor, Badge, Button, Card, Group, Loader, ScrollArea, Stack, Table, Text, Title } from '@mantine/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { Link, useNavigate } from 'react-router'

import { api } from '../api'
import { cuando, n, tamano, titulo } from '../formato'
import { ESTADO_MUDANZA } from '../opciones'
import type { ListaModelos, MudanzaFila, Plan } from '../tipos'

export default function Mudanzas() {
  const navegar = useNavigate()
  const pendientes = useQuery({
    queryKey: ['modelos', 'pendiente=si&por_pagina=500'],
    queryFn: () => api<ListaModelos>('/modelos?pendiente=si&por_pagina=500'),
  })
  const historial = useQuery({
    queryKey: ['mudanzas'],
    queryFn: () => api<{ deshacible: number | null; mudanzas: MudanzaFila[] }>('/mudanzas'),
  })
  const armar = useMutation({
    mutationFn: () => api<Plan>('/mudanzas/plan', { method: 'POST' }),
    onSuccess: (p) => navegar(`/mudanzas/${p.id}`),
  })

  const lista = pendientes.data?.modelos ?? []
  const revisados = lista.filter((m) => m.revisado === 'OK' || m.revisado === 'Corregido')

  return (
    <Stack maw={1000}>
      <Title order={2}>Mudanzas</Title>
      <Text c="dimmed" size="sm">
        Cuando cambias juego, facción, modelo, estudio o subcarpeta, el modelo queda pendiente de mover. Aquí se arma
        el plan (archivo por archivo, sin tocar nada), se revisa y se ejecuta. Nunca se sobrescribe un archivo y la
        última mudanza se puede deshacer.
      </Text>

      <Card withBorder>
        <Group justify="space-between" mb="xs">
          <div>
            <Text fw={500}>Pendientes de mover</Text>
            <Text size="sm" c="dimmed">
              {n(lista.length)} modelos; entran al plan los {n(revisados.length)} marcados OK o Corregido
            </Text>
          </div>
          <Button onClick={() => armar.mutate()} loading={armar.isPending} disabled={revisados.length === 0}>
            Armar plan
          </Button>
        </Group>
        {armar.error && (
          <Alert color="red" variant="light" mb="xs">
            {armar.error.message}
          </Alert>
        )}
        {pendientes.isPending ? (
          <Loader size="sm" />
        ) : lista.length === 0 ? (
          <Text size="sm" c="dimmed">
            Todo está en su lugar.
          </Text>
        ) : (
          <ScrollArea>
            <Table miw={640}>
              <Table.Tbody>
                {lista.map((m) => (
                  <Table.Tr key={m.id}>
                    <Table.Td>
                      <Anchor component={Link} to={`/catalogo/${m.id}`} size="sm">
                        {titulo(m)}
                      </Anchor>
                      {!(m.revisado === 'OK' || m.revisado === 'Corregido') && (
                        <Badge size="xs" ml="xs" variant="outline" color="gray">
                          sin revisar
                        </Badge>
                      )}
                    </Table.Td>
                    <Table.Td>
                      <Text size="xs" c="dimmed" style={{ wordBreak: 'break-all' }}>
                        {m.carpeta}
                      </Text>
                      <Text size="xs" style={{ wordBreak: 'break-all' }}>
                        → {m.ruta_nueva}
                      </Text>
                    </Table.Td>
                    <Table.Td ta="right">
                      <Text size="sm">{tamano(m.bytes)}</Text>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </ScrollArea>
        )}
      </Card>

      <Card withBorder>
        <Text fw={500} mb="xs">
          Historial
        </Text>
        {historial.data?.mudanzas.length === 0 && (
          <Text size="sm" c="dimmed">
            Todavía no hay mudanzas.
          </Text>
        )}
        <Table>
          <Table.Tbody>
            {historial.data?.mudanzas.map((m) => (
              <Table.Tr key={m.id}>
                <Table.Td>
                  <Anchor component={Link} to={`/mudanzas/${m.id}`} size="sm">
                    Mudanza {m.id}
                  </Anchor>
                  <Text size="xs" c="dimmed">
                    {cuando(m.ejecutada ?? m.creada)} · {m.usuario}
                  </Text>
                </Table.Td>
                <Table.Td>
                  <Badge variant="light" color={ESTADO_MUDANZA[m.estado]?.color}>
                    {ESTADO_MUDANZA[m.estado]?.texto ?? m.estado}
                  </Badge>
                  {historial.data?.deshacible === m.id && (
                    <Badge ml="xs" variant="outline" size="xs">
                      se puede deshacer
                    </Badge>
                  )}
                </Table.Td>
                <Table.Td ta="right">
                  <Text size="sm">
                    {n(m.modelos)} modelos · {n(m.archivos ?? 0)} archivos
                  </Text>
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      </Card>
    </Stack>
  )
}
